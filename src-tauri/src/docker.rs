//! Docker containers running *inside* an LXC or VM (issue #65).
//!
//! pxx-dex already manages the guest; this shows what runs in it. There is no
//! Proxmox API for "exec inside an LXC", so everything here goes over the SSH
//! connection to the node that `ssh.rs` opens, and shells out to the node's
//! own `pct exec` / `qm guest exec` to reach into the guest:
//!
//! ```text
//! app --ssh--> node --pct exec / qm guest exec--> guest --> docker CLI
//! ```
//!
//! Deliberately not a `bollard` port. Talking to the Docker Engine API would
//! mean tunnelling the guest's Docker socket out through two hops to save
//! shelling out to a CLI that is already there. `docker ps --format
//! '{{json .}}'` is machine-readable enough for a list, a few actions, and
//! `docker logs`. If this ever needs streaming logs, stats, or exec, that is
//! the point to build the tunnel and pull in `bollard` -- matching
//! dockshell's module rather than reinventing it.
//!
//! No new credentials: this rides the connection's existing SSH config and
//! the session `ssh_console.rs` may already have open.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::connections;
use crate::proxmox::types::GuestKind;
use crate::ssh::{self, ExecOutput, Session, SshSessions};

/// Ceiling on one command, end to end. Every call here is a short,
/// non-interactive `docker` invocation; anything slower than this means the
/// node, the guest, or the daemon is wedged, and a UI waiting forever is
/// worse than an error.
const EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// Cap on `docker logs --tail`. The output crosses the Tauri bridge as one
/// string and lands in a `<pre>`, so an unbounded tail is a way to freeze the
/// webview by asking for a chatty container's whole history.
const MAX_LOG_LINES: u32 = 1000;

/// The six fields `PsLine` actually reads, named explicitly.
///
/// `{{json .}}` would be shorter, but it emits every column `docker ps`
/// knows -- including `Labels`, which for an image that ships a description
/// (portainer's is HTML) is kilobytes per container that this module parses
/// and throws away. Measured against a live guest: one portainer container
/// was 5065 bytes as `{{json .}}` and 644 for four immich containers with
/// this template. That payload crosses SSH on every refresh.
///
/// The cost is that a pre-20.10 daemon, which has no `State` column, fails
/// the whole template rather than omitting one field. `PsLine` keeps its
/// `default`s regardless -- they still cover a missing `Ports` on a
/// container that publishes nothing.
const PS_FORMAT: &str = concat!(
    r#"{"ID":{{json .ID}},"Names":{{json .Names}},"Image":{{json .Image}},"#,
    r#""State":{{json .State}},"Status":{{json .Status}},"Ports":{{json .Ports}}}"#,
);

/// One container as the guest's `docker ps` reported it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    /// `running`, `exited`, `created`, ... -- the machine-readable one.
    pub state: String,
    /// `Up 3 hours`, `Exited (0) 2 days ago` -- the human one.
    pub status: String,
    pub ports: String,
}

/// What `docker ps --format '{{json .}}'` emits, one object per line.
/// `State` and `Ports` are `default`ed: older daemons omit `State`, and a
/// container with no published ports omits `Ports`.
#[derive(Debug, Deserialize)]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State", default)]
    state: String,
    #[serde(rename = "Status", default)]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: String,
}

/// The lifecycle actions this feature exposes. A closed set on purpose --
/// the container reference reaching the node's shell is checked against
/// `valid_container_ref`, and the verb never comes from user text at all.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerAction {
    Start,
    Stop,
    Restart,
}

impl DockerAction {
    fn as_str(self) -> &'static str {
        match self {
            DockerAction::Start => "start",
            DockerAction::Stop => "stop",
            DockerAction::Restart => "restart",
        }
    }
}

/// Wraps a string so a POSIX shell sees it as one literal argument.
///
/// Single-quote everything and escape embedded single quotes the only way
/// `sh` allows: close the quote, emit an escaped quote, reopen. Nothing else
/// -- backslash, `$`, backtick, newline -- has any meaning inside single
/// quotes, so this is total rather than a blocklist.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Whether a container name or id is safe to name in a shell command.
///
/// Belt to `shell_quote`'s braces. Docker's own rule for names is
/// `[a-zA-Z0-9][a-zA-Z0-9_.-]*` and ids are hex, so anything outside that
/// set did not come from `docker ps` -- reject it rather than quote it and
/// hope.
fn valid_container_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Wraps a command so the node runs it *inside* the guest.
///
/// LXC gets `pct exec`, which needs nothing installed in the container. A VM
/// goes through `qm guest exec`, which needs `qemu-guest-agent` running in
/// the guest -- there is no other way in without the guest's own SSH
/// credentials, which this feature deliberately does not ask for.
fn guest_command(kind: GuestKind, vmid: u32, inner: &str) -> String {
    let quoted = shell_quote(inner);
    match kind {
        GuestKind::Lxc => format!("pct exec {vmid} -- /bin/sh -c {quoted}"),
        GuestKind::Qemu => format!(
            "qm guest exec {vmid} --timeout {} -- /bin/sh -c {quoted}",
            EXEC_TIMEOUT.as_secs()
        ),
    }
}

/// `qm guest exec` reports the guest command's result as a JSON envelope on
/// its own stdout rather than passing the streams through, so a VM's output
/// has to be unwrapped before it looks like an LXC's.
#[derive(Debug, Deserialize)]
struct AgentExecResult {
    #[serde(rename = "out-data", default)]
    out_data: String,
    #[serde(rename = "err-data", default)]
    err_data: String,
    #[serde(default)]
    exitcode: i64,
}

/// Turns a `qm guest exec` envelope into the same shape `pct exec` gives.
///
/// A non-zero exit from `qm` itself means the agent never ran the command --
/// guest powered off, or `qemu-guest-agent` not installed -- which is a
/// different failure from "the command ran and returned non-zero", so it is
/// reported rather than folded into the payload.
fn unwrap_agent_output(out: ExecOutput) -> Result<ExecOutput, String> {
    if out.exit_status != 0 {
        let detail = out.stderr.trim();
        return Err(if detail.is_empty() {
            "The QEMU guest agent did not run the command. Is the guest \
             running with qemu-guest-agent installed?"
                .to_string()
        } else {
            format!("The QEMU guest agent could not run the command: {detail}")
        });
    }
    let parsed: AgentExecResult = serde_json::from_str(out.stdout.trim()).map_err(|_| {
        "The QEMU guest agent returned something this app could not read.".to_string()
    })?;
    Ok(ExecOutput {
        stdout: parsed.out_data,
        stderr: parsed.err_data,
        exit_status: parsed.exitcode.clamp(0, i64::from(u32::MAX)) as u32,
    })
}

/// Runs one command on the node over SSH, reusing the connection's open
/// session when there is one.
///
/// A cached session can be dead (the node rebooted, the shell's own bridge
/// dropped it), and there is no cheap way to ask -- so a cached attempt that
/// fails for any reason is retried once on a fresh connection instead of
/// being reported. Only the fresh attempt's failure reaches the user.
async fn exec_on_node(
    app: &tauri::AppHandle,
    sessions: &SshSessions,
    connection_id: &str,
    command: &str,
) -> Result<ExecOutput, String> {
    let target = connections::ssh_target(app, connection_id)?;

    // Cloned out of the map in one statement: the std Mutex guard must not
    // be held across the awaits below.
    let cached = sessions.0.lock().unwrap().get(connection_id).cloned();
    if let Some(session) = cached {
        let attempt = {
            let handle = session.handle.lock().await;
            tokio::time::timeout(EXEC_TIMEOUT, ssh::exec(&handle, command)).await
        };
        if let Ok(Ok(out)) = attempt {
            return Ok(out);
        }
        sessions.0.lock().unwrap().remove(connection_id);
    }

    let known_hosts_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let handle = ssh::connect(
        &target.host,
        target.port,
        &target.user,
        &target.auth(),
        &known_hosts_dir,
    )
    .await
    .map_err(|e| e.to_string())?;
    let session = Session {
        handle: Arc::new(tokio::sync::Mutex::new(handle)),
    };
    sessions
        .0
        .lock()
        .unwrap()
        .insert(connection_id.to_string(), session.clone());

    let handle = session.handle.lock().await;
    tokio::time::timeout(EXEC_TIMEOUT, ssh::exec(&handle, command))
        .await
        .map_err(|_| {
            format!(
                "The command on {} did not finish within {}s.",
                target.host,
                EXEC_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| ssh::describe_shell_error(&e, &target.host, target.port))
}

/// Why `pct exec` never reached the guest, if that is what happened.
///
/// `pct` fails before entering the container -- "container '105' not
/// running!" (exit 255), "Configuration file 'nodes/proxmox/lxc/999.conf'
/// does not exist" (exit 2) -- and writes that to the same stderr a failing
/// `docker` would have used. Folding those into `docker_error` produced
/// "Docker could not list containers: container '105' not running!", which
/// names the wrong subject: Docker was never asked anything.
///
/// Matched on `pct`'s wording rather than its exit codes because the code
/// belongs to whatever `pct` decides to exit with, while these two messages
/// are the two ways in this module's path that the guest is not there. The
/// Docker CLI's own failures say "No such container" and exit 1/125/126/127,
/// so there is no overlap to disambiguate.
///
/// `qm guest exec` needs none of this -- `unwrap_agent_output` already
/// separates the agent's failure from the command's.
fn guest_unreachable(out: &ExecOutput) -> Option<&'static str> {
    if out.exit_status == 0 {
        return None;
    }
    let detail = out.stderr.trim();
    if detail.contains("not running") {
        Some("This guest is not running, so nothing inside it can be reached. Start it first.")
    } else if detail.contains("does not exist") {
        Some("This guest no longer exists on this node. Refresh the guest list.")
    } else {
        None
    }
}

/// Runs one command inside a guest and normalises the result across both
/// guest kinds.
async fn exec_in_guest(
    app: &tauri::AppHandle,
    sessions: &SshSessions,
    connection_id: &str,
    kind: GuestKind,
    vmid: u32,
    inner: &str,
) -> Result<ExecOutput, String> {
    let out = exec_on_node(
        app,
        sessions,
        connection_id,
        &guest_command(kind, vmid, inner),
    )
    .await?;
    match kind {
        GuestKind::Lxc => match guest_unreachable(&out) {
            Some(why) => Err(why.to_string()),
            None => Ok(out),
        },
        GuestKind::Qemu => unwrap_agent_output(out),
    }
}

/// Whether a failed command failed because the guest has no `docker` at all,
/// as opposed to Docker being there and unhappy. POSIX shells exit 127 for
/// "command not found"; the text check catches wrappers that report the same
/// thing with a different code.
fn is_docker_missing(out: &ExecOutput) -> bool {
    out.exit_status == 127 || out.stderr.contains("not found")
}

/// Drops ANSI escape sequences from text bound for the log `<pre>`.
///
/// Containers log to a pipe that many runtimes still colour (portainer's
/// output arrives full of `ESC[90m`), and a `<pre>` renders those as literal
/// `[90m` noise. Stripping beats interpreting: turning them into real colour
/// would mean emitting HTML and rendering it with `v-html`, which makes
/// container log text -- the least trustworthy string in the app -- an
/// injection surface, to win nothing but colour.
///
/// ponytail: handles CSI (`ESC[`...) and OSC (`ESC]`...), which is what
/// colour and title-setting use. Other two-byte escapes lose only the `ESC`
/// and their introducer; a full state machine is the upgrade if some
/// runtime's output ever needs it.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameter and intermediate bytes, then one final byte in
            // the `@`..`~` range that ends the sequence.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: runs until BEL or the two-character ESC `\` terminator.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn parse_ps(stdout: &str) -> Vec<DockerContainer> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // A single unparseable line (a daemon warning printed to stdout, a
        // format field a newer daemon added) must not sink the whole list.
        .filter_map(|line| serde_json::from_str::<PsLine>(line).ok())
        .map(|p| DockerContainer {
            id: p.id,
            name: p.names,
            image: p.image,
            state: p.state,
            status: p.status,
            ports: p.ports,
        })
        .collect()
}

/// Every container in a guest, running or not.
///
/// `Ok(None)` means the guest is reachable but has no `docker` -- the caller
/// hides the Docker section rather than showing an error, which is what
/// makes this double as the availability probe.
#[tauri::command]
pub async fn docker_ps(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
) -> Result<Option<Vec<DockerContainer>>, String> {
    let out = exec_in_guest(
        &app,
        &sessions,
        &connection_id,
        kind,
        vmid,
        &format!("docker ps -a --format '{PS_FORMAT}'"),
    )
    .await?;
    if is_docker_missing(&out) {
        return Ok(None);
    }
    if out.exit_status != 0 {
        return Err(docker_error(&out, "list containers"));
    }
    Ok(Some(parse_ps(&out.stdout)))
}

/// start / stop / restart one container.
#[tauri::command]
pub async fn docker_action(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    container: String,
    action: DockerAction,
) -> Result<(), String> {
    if !valid_container_ref(&container) {
        return Err("That is not a valid container name or id.".to_string());
    }
    let inner = format!("docker {} {}", action.as_str(), shell_quote(&container));
    let out = exec_in_guest(&app, &sessions, &connection_id, kind, vmid, &inner).await?;
    if out.exit_status != 0 {
        return Err(docker_error(&out, action.as_str()));
    }
    Ok(())
}

/// The last `tail` lines of a container's log, stderr folded in so the order
/// matches what `docker logs` shows in a terminal.
#[tauri::command]
pub async fn docker_logs(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    container: String,
    tail: u32,
) -> Result<String, String> {
    if !valid_container_ref(&container) {
        return Err("That is not a valid container name or id.".to_string());
    }
    let tail = tail.clamp(1, MAX_LOG_LINES);
    let inner = format!("docker logs --tail {tail} {} 2>&1", shell_quote(&container));
    let out = exec_in_guest(&app, &sessions, &connection_id, kind, vmid, &inner).await?;
    if out.exit_status != 0 && out.stdout.trim().is_empty() {
        return Err(docker_error(&out, "read the log"));
    }
    Ok(strip_ansi(&out.stdout))
}

/// A failed `docker` invocation, as a sentence. Falls back to naming the
/// attempted action when the daemon said nothing useful, so the UI never
/// shows a bare exit code.
fn docker_error(out: &ExecOutput, what: &str) -> String {
    let detail = out.stderr.trim();
    if detail.is_empty() {
        format!("Docker could not {what} (exit {}).", out.exit_status)
    } else {
        format!("Docker could not {what}: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_neutralises_an_embedded_single_quote() {
        assert_eq!(shell_quote("plain"), "'plain'");
        // The classic break-out attempt: the quote must be escaped, and the
        // result must still be a single quoted word to the shell.
        assert_eq!(shell_quote("a'; rm -rf /; '"), r"'a'\''; rm -rf /; '\'''");
    }

    #[test]
    fn shell_quote_leaves_shell_metacharacters_inert() {
        for probe in ["$(id)", "`id`", "a && b", "a | b", "a\nb", "a\\b"] {
            let quoted = shell_quote(probe);
            assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
            // No unescaped quote can appear inside, so nothing can end the
            // literal early and start being interpreted.
            assert!(!quoted[1..quoted.len() - 1].contains('\''));
        }
    }

    #[test]
    fn container_refs_outside_dockers_own_charset_are_rejected() {
        assert!(valid_container_ref("web"));
        assert!(valid_container_ref("my_app.1-old"));
        assert!(valid_container_ref("3f4a9c1b2d5e"));

        assert!(!valid_container_ref(""));
        assert!(!valid_container_ref("-leading-dash"));
        assert!(!valid_container_ref("web; rm -rf /"));
        assert!(!valid_container_ref("web$(id)"));
        assert!(!valid_container_ref("web name"));
        assert!(!valid_container_ref(&"a".repeat(129)));
    }

    #[test]
    fn lxc_goes_through_pct_and_qemu_through_the_guest_agent() {
        let lxc = guest_command(GuestKind::Lxc, 101, "docker ps");
        assert!(lxc.starts_with("pct exec 101 -- /bin/sh -c "));
        assert!(lxc.contains("'docker ps'"));

        let qemu = guest_command(GuestKind::Qemu, 202, "docker ps");
        assert!(qemu.starts_with("qm guest exec 202 "));
        assert!(qemu.contains("--timeout"));
        assert!(qemu.contains("'docker ps'"));
    }

    #[test]
    fn the_ps_format_argument_survives_the_two_shell_hops() {
        // The template contains spaces, braces and double quotes, so it is
        // quoted in the inner command and must come back out of the outer
        // quoting intact rather than being split into several arguments.
        let inner = format!("docker ps -a --format '{PS_FORMAT}'");
        let cmd = guest_command(GuestKind::Lxc, 100, &inner);
        assert!(
            cmd.contains(&format!(r"--format '\''{PS_FORMAT}'\''")),
            "{cmd}"
        );
        // No bare single quote can survive inside, or the template would end
        // the shell word early.
        assert!(!PS_FORMAT.contains('\''), "{PS_FORMAT}");
    }

    #[test]
    fn the_ps_template_asks_for_exactly_the_fields_that_are_parsed() {
        // Drift guard: a field added to `PsLine` without being added here
        // would silently always be its `default`.
        for field in ["ID", "Names", "Image", "State", "Status", "Ports"] {
            assert!(
                PS_FORMAT.contains(&format!("{{{{json .{field}}}}}")),
                "{field}"
            );
        }
        // What the template exists to avoid asking for.
        assert!(!PS_FORMAT.contains("Labels"), "{PS_FORMAT}");
        // And it must still parse as the JSON object `parse_ps` expects,
        // once the daemon has substituted the values.
        let rendered = PS_FORMAT
            .replace("{{json .ID}}", r#""abc""#)
            .replace("{{json .Names}}", r#""web""#)
            .replace("{{json .Image}}", r#""nginx""#)
            .replace("{{json .State}}", r#""running""#)
            .replace("{{json .Status}}", r#""Up 2 hours""#)
            .replace("{{json .Ports}}", r#""""#);
        let got = parse_ps(&rendered);
        assert_eq!(got.len(), 1, "{rendered}");
        assert_eq!(got[0].name, "web");
        assert_eq!(got[0].state, "running");
    }

    #[test]
    fn colour_codes_are_stripped_out_of_a_log() {
        // A real portainer log line: SGR colour around each field.
        let raw = "\x1b[90m2026/07/29 09:24AM\x1b[0m \x1b[32mINF\x1b[0m starting Portainer";
        assert_eq!(strip_ansi(raw), "2026/07/29 09:24AM INF starting Portainer");
    }

    #[test]
    fn stripping_leaves_ordinary_log_text_byte_for_byte() {
        for probe in [
            "",
            "plain line\n",
            "json {\"a\": [1, 2]} and a bracket ] and a [90m that is not an escape",
            "unicode: caffè 中文 🐳\n",
        ] {
            assert_eq!(strip_ansi(probe), probe);
        }
    }

    #[test]
    fn a_title_setting_escape_does_not_swallow_the_rest_of_the_log() {
        // OSC ends at BEL; everything after it must survive.
        assert_eq!(strip_ansi("\x1b]0;a title\x07after"), "after");
        // And at the ESC `\` string terminator.
        assert_eq!(strip_ansi("\x1b]0;a title\x1b\\after"), "after");
        // An unterminated CSI at the very end must not panic or loop.
        assert_eq!(strip_ansi("done\x1b["), "done");
        assert_eq!(strip_ansi("done\x1b"), "done");
    }

    #[test]
    fn pct_failing_to_enter_the_guest_is_not_reported_as_a_docker_failure() {
        // The two ways this module's path finds no guest, verbatim from a
        // live node.
        let stopped = ExecOutput {
            stdout: String::new(),
            stderr: "container '105' not running!\n".into(),
            exit_status: 255,
        };
        let why = guest_unreachable(&stopped).expect("stopped guest is recognised");
        assert!(why.contains("not running"), "{why}");
        // The old wording blamed Docker for it.
        assert!(!docker_error(&stopped, "list containers").contains("not running!\n"));

        let gone = ExecOutput {
            stdout: String::new(),
            stderr: "Configuration file 'nodes/proxmox/lxc/999.conf' does not exist\n".into(),
            exit_status: 2,
        };
        assert!(guest_unreachable(&gone).is_some());
    }

    #[test]
    fn dockers_own_failures_still_reach_the_user_as_docker_failures() {
        for (stderr, status) in [
            ("Error response from daemon: No such container: web", 1),
            (
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock.",
                1,
            ),
            ("/bin/sh: docker: not found", 127),
        ] {
            let out = ExecOutput {
                stdout: String::new(),
                stderr: stderr.into(),
                exit_status: status,
            };
            assert!(guest_unreachable(&out).is_none(), "{stderr}");
        }
        // A successful command is never "unreachable", whatever it printed.
        let ok = ExecOutput {
            stdout: String::new(),
            stderr: "warning: something does not exist".into(),
            exit_status: 0,
        };
        assert!(guest_unreachable(&ok).is_none());
    }

    #[test]
    fn parses_one_container_per_line() {
        let stdout = concat!(
            r#"{"ID":"abc123","Names":"web","Image":"nginx","State":"running","Status":"Up 2 hours","Ports":"0.0.0.0:80->80/tcp"}"#,
            "\n",
            r#"{"ID":"def456","Names":"db","Image":"postgres:16","State":"exited","Status":"Exited (0) 1 day ago","Ports":""}"#,
            "\n",
        );
        let got = parse_ps(stdout);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].name, "web");
        assert_eq!(got[0].state, "running");
        assert_eq!(got[0].ports, "0.0.0.0:80->80/tcp");
        assert_eq!(got[1].image, "postgres:16");
    }

    #[test]
    fn a_junk_line_is_dropped_without_losing_the_rest() {
        let stdout = concat!(
            "WARNING: something the daemon printed\n",
            r#"{"ID":"abc","Names":"web","Image":"nginx","State":"running","Status":"Up","Ports":""}"#,
            "\n\n",
        );
        let got = parse_ps(stdout);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "web");
    }

    #[test]
    fn missing_optional_ps_fields_do_not_drop_the_container() {
        // An older daemon that emits no `State`/`Ports` still yields a row.
        let stdout = r#"{"ID":"abc","Names":"web","Image":"nginx","Status":"Up"}"#;
        let got = parse_ps(stdout);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].state, "");
    }

    #[test]
    fn empty_output_is_an_empty_list_not_an_error() {
        assert!(parse_ps("").is_empty());
        assert!(parse_ps("\n\n").is_empty());
    }

    #[test]
    fn a_guest_without_docker_is_detected_rather_than_reported_as_an_error() {
        let out = ExecOutput {
            stdout: String::new(),
            stderr: "/bin/sh: docker: not found".into(),
            exit_status: 127,
        };
        assert!(is_docker_missing(&out));

        let unhappy = ExecOutput {
            stdout: String::new(),
            stderr: "Cannot connect to the Docker daemon".into(),
            exit_status: 1,
        };
        assert!(!is_docker_missing(&unhappy));
    }

    #[test]
    fn the_guest_agent_envelope_is_unwrapped_into_plain_output() {
        let out = ExecOutput {
            stdout: r#"{"exitcode":0,"out-data":"hello\n","err-data":"","exited":1}"#.into(),
            stderr: String::new(),
            exit_status: 0,
        };
        let got = unwrap_agent_output(out).expect("well-formed envelope");
        assert_eq!(got.stdout, "hello\n");
        assert_eq!(got.exit_status, 0);
    }

    #[test]
    fn the_guest_agents_own_failure_is_not_reported_as_the_commands_failure() {
        let out = ExecOutput {
            stdout: String::new(),
            stderr: "QEMU guest agent is not running".into(),
            exit_status: 255,
        };
        let err = unwrap_agent_output(out).expect_err("qm itself failed");
        assert!(err.contains("guest agent"), "{err}");
    }

    #[test]
    fn an_unreadable_envelope_is_an_error_not_a_silent_empty_result() {
        let out = ExecOutput {
            stdout: "not json at all".into(),
            stderr: String::new(),
            exit_status: 0,
        };
        assert!(unwrap_agent_output(out).is_err());
    }

    #[test]
    fn a_negative_agent_exit_code_does_not_wrap_around_to_a_huge_number() {
        let out = ExecOutput {
            stdout: r#"{"exitcode":-1,"out-data":"","err-data":"boom"}"#.into(),
            stderr: String::new(),
            exit_status: 0,
        };
        let got = unwrap_agent_output(out).expect("well-formed envelope");
        assert_eq!(got.exit_status, 0);
    }

    #[test]
    fn docker_errors_read_as_sentences_even_with_no_stderr() {
        let quiet = ExecOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_status: 3,
        };
        let msg = docker_error(&quiet, "stop");
        assert!(msg.contains("stop") && msg.contains('3'));

        let loud = ExecOutput {
            stdout: String::new(),
            stderr: "No such container: web".into(),
            exit_status: 1,
        };
        assert!(docker_error(&loud, "stop").contains("No such container"));
    }
}
