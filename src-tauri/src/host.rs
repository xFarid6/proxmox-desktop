//! Read-only facts about a plain SSH host: what is listening, what systemd is
//! running (issue #104), and which of those listeners is a media stream (#106).
//!
//! Unlike [`crate::docker`], there is no guest hop here — an "SSH host"
//! connection (#102) *is* the machine, so every command is one
//! [`ssh::exec_on_connection`] call plus a parser:
//!
//! ```text
//! app --ssh--> host --> ss / systemctl / curl
//! ```
//!
//! Every command is a pure read. Starting, stopping or restarting a unit is
//! deliberately out of scope for #104.

use std::collections::HashSet;

use serde::Serialize;

use crate::ssh::{self, ExecOutput, SshSessions};

/// One socket in `ss`'s listening set.
///
/// `process`/`pid` are optional because `ss` only names the owning process
/// for sockets the calling user owns — as a non-root SSH user the whole
/// `users:(...)` column is simply absent. That is the normal case for a
/// hardened host, not an error, so the port is still reported.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeningPort {
    /// `tcp` or `udp`, as `ss` labels it.
    pub proto: String,
    /// The local address the socket is bound to, verbatim: `0.0.0.0`,
    /// `[::]`, `*`, or a specific address including any `%iface` scope.
    pub address: String,
    pub port: u16,
    pub process: Option<String>,
    pub pid: Option<u32>,
}

/// One systemd service unit. `active` is the high-level state
/// (`active`/`failed`) and `sub` the detailed one (`running`/`failed`/`exited`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUnit {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

/// One listening port that answered [`STREAM_PATH`] with a media content
/// type (#106).
///
/// `path` is carried rather than re-derived by the viewer: it is the path
/// that was actually probed, so the URL the app renders is the URL that was
/// proven to answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEndpoint {
    pub port: u16,
    pub path: String,
    /// The `Content-Type` header verbatim, boundary included.
    pub content_type: String,
    /// `stream` for `multipart/x-mixed-replace`, `snapshot` for a still image.
    pub kind: String,
    /// The owning process from `ss`, when it named one — see [`ListeningPort`].
    pub process: Option<String>,
}

/// The one path probed on every candidate port. mjpg_streamer's, which is the
/// driving case, and the shape the issue names.
///
/// ponytail: one path, not a list. A server that serves its stream somewhere
/// else is not detected; probing N paths multiplies the wall clock below by N
/// for a guess. Add a second path when a real host needs one.
const STREAM_PATH: &str = "/?action=stream";

/// Seconds curl waits per port. A live MJPEG stream never closes the
/// connection, so this timeout — not the response — is what ends every
/// successful probe. The headers are already on stdout by then.
const PROBE_TIMEOUT_SECS: u32 = 2;

/// Cap on ports probed in one call. The probes are serial, so the worst case
/// is `MAX_PROBES * PROBE_TIMEOUT_SECS` = 20s, which has to stay under
/// [`ssh::EXEC_TIMEOUT`] (30s) or a busy host times out the whole tab.
const MAX_PROBES: usize = 10;

/// Separator echoed before each probe. `@@ ` cannot start an HTTP header
/// line, so splitting on it cannot cut a response in half.
const PROBE_MARK: &str = "@@ ";

/// The one command both tabs list sockets with.
///
/// `-u` is load-bearing even for the TCP-only stream probe: `ss` prints the
/// `Netid` column only when more than one protocol is asked for, and
/// [`parse_ports`] reads the protocol from field 0 and the local address from
/// field 4. Ask for `-tlnp` alone and every row shifts left — field 0 becomes
/// `LISTEN` and field 4 becomes the *peer* address — so nothing matches the
/// `tcp` filter and the stream tab silently finds no endpoints on any host.
/// UDP rows are filtered out in [`probe_targets`] instead, which costs
/// nothing.
pub(crate) const SS_LISTENING: &str = "ss -tulnp";

/// Whether a failed command failed because the tool is not installed at all,
/// as opposed to the tool being there and unhappy. Same reasoning as
/// `docker.rs`'s `is_docker_missing`: POSIX shells exit 127 for "command not
/// found", and the text check catches wrappers that use a different code.
fn tool_missing(out: &ExecOutput) -> bool {
    out.exit_status == 127 || out.stderr.contains("not found")
}

/// Pulls the process name and pid out of `ss`'s
/// `users:(("name",pid=N,fd=M))` column. `None` when the column is absent
/// (non-root) or shaped differently than expected.
///
/// ponytail: reads only the first entry. A socket shared by several processes
/// lists them all; showing the first is what the table has room for.
fn parse_user_field(line: &str) -> Option<(String, u32)> {
    let rest = line.split_once("users:((\"")?.1;
    let (name, rest) = rest.split_once('"')?;
    let digits = rest.split_once("pid=")?.1;
    let pid = digits
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((name.to_string(), pid))
}

/// Splits `ss`'s `Local Address:Port` field. The address half is kept
/// verbatim — brackets, `*` and `%iface` scopes included — because rewriting
/// it would only lose information the reader recognises.
fn split_addr_port(field: &str) -> Option<(String, u16)> {
    let (addr, port) = field.rsplit_once(':')?;
    Some((addr.to_string(), port.parse().ok()?))
}

pub(crate) fn parse_ports(stdout: &str) -> Vec<ListeningPort> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // The header, when `ss` printed one. Its own `Local Address:Port`
        // column would not parse anyway, but skipping it by name says why.
        .filter(|line| !line.starts_with("Netid"))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // netid state recv-q send-q local peer [users:...]
            let (address, port) = split_addr_port(fields.get(4)?)?;
            let (process, pid) = match parse_user_field(line) {
                Some((name, pid)) => (Some(name), Some(pid)),
                None => (None, None),
            };
            Some(ListeningPort {
                proto: fields.first()?.to_string(),
                address,
                port,
                process,
                pid,
            })
        })
        .collect()
}

fn parse_units(stdout: &str) -> Vec<ServiceUnit> {
    stdout
        .lines()
        // `--plain` drops systemd's bullet, but a version that still prints
        // one must not shift every column by a field.
        .map(|line| line.trim().trim_start_matches(['●', '*', ' ']))
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.to_string();
            let load = fields.next()?.to_string();
            let active = fields.next()?.to_string();
            let sub = fields.next()?.to_string();
            Some(ServiceUnit {
                name,
                load,
                active,
                sub,
                // The description is the rest of the line, and it has spaces
                // in it ("Regular background program processing daemon").
                description: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// The address curl should dial for a socket bound to `address`.
///
/// A wildcard bind is dialled over loopback: the probe answers "is this
/// service alive at all", which is deliberately a different question from
/// "can this desktop reach it". The viewer answers the second one by failing
/// to load, and that split is the point — the 2026-08-02 outage was a service
/// that was up with the network to it broken.
fn probe_address(address: &str) -> &str {
    match address {
        "0.0.0.0" | "*" | "[::]" | "::" => "127.0.0.1",
        specific => specific,
    }
}

/// The ports worth probing: TCP, one entry per port number, SSH excluded.
///
/// Port 22 is skipped because curl talking HTTP to sshd only sits there until
/// the timeout — a guaranteed two wasted seconds on every host.
fn probe_targets(ports: &[ListeningPort]) -> Vec<ListeningPort> {
    let mut seen = HashSet::new();
    ports
        .iter()
        .filter(|p| p.proto == "tcp" && p.port != 22)
        // The same service usually listens twice, on 0.0.0.0 and on [::].
        .filter(|p| seen.insert(p.port))
        .take(MAX_PROBES)
        .cloned()
        .collect()
}

fn probe_command(targets: &[ListeningPort]) -> String {
    let list = targets
        .iter()
        .map(|p| format!("{}:{}", probe_address(&p.address), p.port))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "for hp in {list}; do echo \"{PROBE_MARK}$hp\"; \
         curl -s -m {PROBE_TIMEOUT_SECS} -o /dev/null -D - \"http://$hp{STREAM_PATH}\"; done"
    )
}

/// Classifies one probe's response headers.
///
/// `None` for anything that is not a media endpoint, which is the common case
/// — most listening ports are an API, a database or nothing HTTP at all.
/// The status line is checked so that a 404 page does not get read as content.
fn endpoint_kind(block: &str) -> Option<(String, &'static str)> {
    let mut lines = block.lines().map(str::trim).filter(|l| !l.is_empty());
    // `HTTP/1.0 200 OK`, and it has to be a 2xx: an error page carries a
    // content type of its own, and mjpg_streamer answers 400 with one to
    // anything it does not understand — a HEAD request, for instance, which
    // is why the probe is a GET.
    let status = lines.next()?;
    let code = status.strip_prefix("HTTP/")?.split_whitespace().nth(1)?;
    if !code.starts_with('2') {
        return None;
    }
    // Case matters here in practice, not just in theory: MJPG-Streamer/0.2
    // sends `Content-Type` for the stream and `Content-type` for a snapshot.
    let header = lines.find(|l| l.to_ascii_lowercase().starts_with("content-type:"))?;
    let value = header.split_once(':')?.1.trim();
    let lower = value.to_ascii_lowercase();
    let kind = if lower.contains("multipart/x-mixed-replace") {
        "stream"
    } else if lower.starts_with("image/") {
        // The server ignored the query and served a still. Worth showing —
        // one frame still answers "is the camera alive" — but not as a stream.
        "snapshot"
    } else {
        return None;
    };
    Some((value.to_string(), kind))
}

fn parse_probes(stdout: &str, targets: &[ListeningPort]) -> Vec<StreamEndpoint> {
    stdout
        .split(PROBE_MARK)
        // Anything before the first mark is not a probe.
        .skip(1)
        .filter_map(|chunk| {
            let (marker, headers) = chunk.split_once('\n')?;
            let port: u16 = marker.trim().rsplit(':').next()?.parse().ok()?;
            let (content_type, kind) = endpoint_kind(headers)?;
            Some(StreamEndpoint {
                port,
                path: STREAM_PATH.to_string(),
                content_type,
                kind: kind.to_string(),
                process: targets
                    .iter()
                    .find(|t| t.port == port)
                    .and_then(|t| t.process.clone()),
            })
        })
        .collect()
}

/// Every listening TCP/UDP socket on the host.
///
/// `Ok(None)` means the host is reachable but has no `ss` — the caller says
/// so rather than showing an error, the same convention `docker_ps` uses.
///
/// ponytail: `-H` (skip the header) exists but is not in every iproute2 old
/// enough to be in the field, so the header is skipped in the parser instead
/// and this stays one command with no version fallback.
#[tauri::command]
pub async fn host_ports(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
) -> Result<Option<Vec<ListeningPort>>, String> {
    let out = ssh::exec_on_connection(&app, &sessions, &connection_id, SS_LISTENING).await?;
    if tool_missing(&out) {
        return Ok(None);
    }
    if out.exit_status != 0 {
        return Err(format!(
            "Could not list listening ports: {}",
            out.stderr.trim()
        ));
    }
    Ok(Some(parse_ports(&out.stdout)))
}

/// Running and failed systemd service units. `Ok(None)` means the host has no
/// `systemctl` (a non-systemd distro), which is not an error either.
#[tauri::command]
pub async fn host_services(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
) -> Result<Option<Vec<ServiceUnit>>, String> {
    let out = ssh::exec_on_connection(
        &app,
        &sessions,
        &connection_id,
        "systemctl list-units --type=service --state=running,failed \
         --no-pager --plain --no-legend",
    )
    .await?;
    if tool_missing(&out) {
        return Ok(None);
    }
    if out.exit_status != 0 {
        return Err(format!("Could not list services: {}", out.stderr.trim()));
    }
    Ok(Some(parse_units(&out.stdout)))
}

/// Media endpoints among the host's listening ports (#106).
///
/// Two round trips on the same cached session: `ss` for the candidates, then
/// one shell loop that curls each. `Ok(None)` means the host is missing `ss`
/// or `curl`, so nothing *can* be detected — the caller says so rather than
/// showing an error, the same convention [`host_ports`] uses.
///
/// An empty list is the honest answer for a host with no media endpoint, and
/// is not the same as `None`.
#[tauri::command]
pub async fn host_streams(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
) -> Result<Option<Vec<StreamEndpoint>>, String> {
    let listening = ssh::exec_on_connection(&app, &sessions, &connection_id, SS_LISTENING).await?;
    if tool_missing(&listening) {
        return Ok(None);
    }
    if listening.exit_status != 0 {
        return Err(format!(
            "Could not list listening ports: {}",
            listening.stderr.trim()
        ));
    }
    let targets = probe_targets(&parse_ports(&listening.stdout));
    if targets.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let probed =
        ssh::exec_on_connection(&app, &sessions, &connection_id, &probe_command(&targets)).await?;
    if tool_missing(&probed) {
        return Ok(None);
    }
    // The exit status is deliberately not checked: curl exits 28 on the
    // timeout that ends every *successful* stream probe, and the loop's status
    // is the last probe's, whichever port that happened to be.
    Ok(Some(parse_probes(&probed.stdout, &targets)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from wyse-server (iproute2-6.15.0) on 2026-08-03 as root.
    /// Kept verbatim, column padding included: the padding is what a naive
    /// fixed-width parse would trip over.
    const SS_ROOT: &str = r#"
udp UNCONN 0      0                                 192.168.1.105:68    0.0.0.0:* users:(("dhcpcd",pid=1200,fd=3))
udp UNCONN 0      960                                     0.0.0.0:41641 0.0.0.0:* users:(("tailscaled",pid=781,fd=21))
udp UNCONN 0      0      [fdec:6e0f:d338:e268:5877:9122:47cd:be9]:546      [::]:* users:(("dhcpcd",pid=1183,fd=3))
udp UNCONN 0      0            [fe80::9bbf:e90d:264a:ae0f]%enp1s0:546      [::]:* users:(("dhcpcd",pid=987,fd=3))
tcp LISTEN 0      128                                     0.0.0.0:22    0.0.0.0:* users:(("sshd",pid=803,fd=6))
tcp LISTEN 0      4096                                    0.0.0.0:8082  0.0.0.0:* users:(("docker-proxy",pid=1269910,fd=8))
tcp LISTEN 0      128                                        [::]:22       [::]:* users:(("sshd",pid=803,fd=7))
tcp LISTEN 0      4096                                          *:9090        *:* users:(("systemd",pid=1,fd=60))
"#;

    /// The same host as the `nobody` user: `ss` drops the whole `users:`
    /// column rather than reporting an error.
    const SS_NON_ROOT: &str = r#"
udp UNCONN 0      0                                 192.168.1.105:68    0.0.0.0:*
tcp LISTEN 0      128                                     0.0.0.0:22    0.0.0.0:*
tcp LISTEN 0      4096                                          *:9090        *:*
"#;

    /// The same host as `ss -tlnp` — one protocol, so `ss` drops the `Netid`
    /// column and prints a `State`-first header instead. Captured from
    /// wyse-server on 2026-08-03; this is the shape the stream probe used to
    /// ask for, and every field it needs is one column to the left.
    const SS_TCP_ONLY: &str = r#"
State  Recv-Q Send-Q               Local Address:Port  Peer Address:PortProcess
LISTEN 0      128                        0.0.0.0:22         0.0.0.0:*    users:(("sshd",pid=803,fd=6))
LISTEN 0      4096                       0.0.0.0:8082       0.0.0.0:*    users:(("docker-proxy",pid=1269910,fd=8))
"#;

    /// wyse-server's units, plus a failed one in the standard shape —
    /// nothing was failing at capture time, and the failed row is the whole
    /// reason this tab exists.
    const SYSTEMCTL: &str = r#"
containerd.service        loaded active running containerd container runtime
cron.service              loaded active running Regular background program processing daemon
docker.service            loaded active running Docker Application Container Engine
mjpg-streamer.service     loaded failed failed  MJPEG webcam streamer
ssh.service               loaded active running OpenBSD Secure Shell server
user@0.service            loaded active running User Manager for UID 0
"#;

    /// Both tabs must keep asking for two protocols. Dropping `-u` costs the
    /// `Netid` column, which shifts every field `parse_ports` reads: the
    /// stream tab then found no `tcp` rows at all and reported "no media
    /// endpoint" on a host that was serving one -- a silent wrong answer, not
    /// an error. Live proof: wyse-server's mjpg-streamer on 8082 (2026-08-03).
    #[test]
    fn listing_command_asks_for_both_protocols() {
        assert!(
            SS_LISTENING.contains("-tu") || SS_LISTENING.contains("-ut"),
            "single-protocol `ss` drops the Netid column: {SS_LISTENING}"
        );

        // Why it matters, rather than just that it does.
        let shifted = parse_ports(SS_TCP_ONLY);
        assert!(
            !shifted.iter().any(|p| p.proto == "tcp"),
            "single-protocol output cannot yield tcp rows: {shifted:?}"
        );
        assert!(
            probe_targets(&shifted).is_empty(),
            "and so the stream probe silently has nothing to probe"
        );

        // The command actually used does yield the streaming port.
        let ok = probe_targets(&parse_ports(SS_ROOT));
        assert!(ok.iter().any(|p| p.port == 8082), "found mjpg-streamer");
    }

    #[test]
    fn reads_proto_address_and_port_from_real_output() {
        let ports = parse_ports(SS_ROOT);
        assert_eq!(ports.len(), 8, "every captured row parsed");
        let ssh = &ports[4];
        assert_eq!(ssh.proto, "tcp");
        assert_eq!(ssh.address, "0.0.0.0");
        assert_eq!(ssh.port, 22);
        assert_eq!(ssh.process.as_deref(), Some("sshd"));
        assert_eq!(ssh.pid, Some(803));
    }

    #[test]
    fn keeps_ipv6_addresses_whole() {
        // rsplit_once(':') is the whole trick -- splitting on the first colon
        // would cut `[fdec:...]` in half.
        let ports = parse_ports(SS_ROOT);
        let v6: Vec<_> = ports
            .iter()
            .filter(|p| p.address.starts_with('['))
            .collect();
        assert_eq!(v6.len(), 3);
        assert_eq!(v6[0].address, "[fdec:6e0f:d338:e268:5877:9122:47cd:be9]");
        assert_eq!(v6[0].port, 546);
        // A link-local keeps its interface scope.
        assert_eq!(v6[1].address, "[fe80::9bbf:e90d:264a:ae0f]%enp1s0");
    }

    #[test]
    fn handles_the_family_wildcard_address() {
        let ports = parse_ports(SS_ROOT);
        let wildcard = ports.iter().find(|p| p.port == 9090).expect("*:9090 row");
        assert_eq!(wildcard.address, "*");
        assert_eq!(wildcard.process.as_deref(), Some("systemd"));
    }

    #[test]
    fn a_non_root_capture_lists_ports_with_no_process() {
        // The acceptance criterion "degrades when not root": ports still
        // appear, the process column is just blank.
        let ports = parse_ports(SS_NON_ROOT);
        assert_eq!(ports.len(), 3);
        assert!(ports.iter().all(|p| p.process.is_none() && p.pid.is_none()));
        assert_eq!(ports[1].port, 22);
    }

    #[test]
    fn skips_a_header_when_ss_printed_one() {
        let with_header = format!(
            "Netid State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process\n{}",
            SS_ROOT.trim()
        );
        assert_eq!(parse_ports(&with_header).len(), 8);
    }

    #[test]
    fn empty_output_is_an_empty_list_not_an_error() {
        assert!(parse_ports("").is_empty());
        assert!(parse_ports("\n  \n").is_empty());
        assert!(parse_units("").is_empty());
    }

    #[test]
    fn reads_units_with_multi_word_descriptions() {
        let units = parse_units(SYSTEMCTL);
        assert_eq!(units.len(), 6);
        let cron = &units[1];
        assert_eq!(cron.name, "cron.service");
        assert_eq!(cron.active, "active");
        assert_eq!(cron.sub, "running");
        assert_eq!(
            cron.description,
            "Regular background program processing daemon"
        );
    }

    #[test]
    fn reads_a_failed_unit() {
        let units = parse_units(SYSTEMCTL);
        let failed = units
            .iter()
            .find(|u| u.active == "failed")
            .expect("failed unit");
        assert_eq!(failed.name, "mjpg-streamer.service");
        assert_eq!(failed.sub, "failed");
    }

    #[test]
    fn a_leading_bullet_does_not_shift_the_columns() {
        // Some systemd versions still mark a failed unit with `●` even under
        // --plain; without trimming it, every field would land one to the left.
        let units =
            parse_units("● mjpg-streamer.service loaded failed failed MJPEG webcam streamer");
        assert_eq!(units[0].name, "mjpg-streamer.service");
        assert_eq!(units[0].active, "failed");
    }

    /// Captured from wyse-server on 2026-08-03, running mjpg_streamer for the
    /// physical webcam behind a published docker port. Three ports were
    /// probed: the stream, a Cockpit that answers HTML, and one that refused
    /// the connection and so produced no headers at all.
    const PROBES: &str = r#"@@ 127.0.0.1:8082
HTTP/1.0 200 OK
Access-Control-Allow-Origin: *
Connection: close
Server: MJPG-Streamer/0.2
Cache-Control: no-store, no-cache, must-revalidate, pre-check=0, post-check=0, max-age=0
Pragma: no-cache
Expires: Mon, 3 Jan 2000 12:34:56 GMT
Content-Type: multipart/x-mixed-replace;boundary=boundarydonotcross

@@ 127.0.0.1:9090
HTTP/1.1 200 OK
Content-Type: text/html
Content-Security-Policy: connect-src 'self'

@@ 127.0.0.1:5432
"#;

    #[test]
    fn finds_the_mjpeg_stream_and_nothing_else() {
        let targets = probe_targets(&parse_ports(SS_ROOT));
        let found = parse_probes(PROBES, &targets);
        assert_eq!(found.len(), 1, "only the multipart port is a stream");
        let cam = &found[0];
        assert_eq!(cam.port, 8082);
        assert_eq!(cam.kind, "stream");
        assert_eq!(cam.path, "/?action=stream");
        assert_eq!(
            cam.content_type,
            "multipart/x-mixed-replace;boundary=boundarydonotcross"
        );
        // The port came from `ss`, so the process it named comes along.
        assert_eq!(cam.process.as_deref(), Some("docker-proxy"));
    }

    #[test]
    fn a_still_image_is_a_snapshot_not_a_stream() {
        // Some servers ignore ?action= and just serve a frame. Still worth
        // showing, but the viewer must not expect it to keep updating.
        let block = "HTTP/1.0 200 OK\nServer: MJPG-Streamer/0.2\nContent-type: image/jpeg\n";
        assert_eq!(
            endpoint_kind(block),
            Some(("image/jpeg".to_string(), "snapshot"))
        );
    }

    #[test]
    fn a_content_type_is_matched_whatever_its_case() {
        // MJPG-Streamer/0.2 really does send `Content-Type` for the stream
        // and `Content-type` for a snapshot, in the same process.
        let upper = "HTTP/1.0 200 OK\nCONTENT-TYPE: multipart/x-mixed-replace;boundary=x\n";
        assert_eq!(endpoint_kind(upper).unwrap().1, "stream");
    }

    #[test]
    fn a_non_media_or_dead_port_is_not_an_endpoint() {
        assert_eq!(
            endpoint_kind("HTTP/1.1 200 OK\nContent-Type: text/html\n"),
            None
        );
        // Connection refused: curl printed no headers at all.
        assert_eq!(endpoint_kind(""), None);
        // An error page must not be read as content just because it has one.
        assert_eq!(
            endpoint_kind("HTTP/1.1 404 Not Found\nContent-Type: image/png\n"),
            None,
            "a 404 body is not the endpoint"
        );
        // Something that is not HTTP at all -- an SMTP or SSH banner.
        assert_eq!(endpoint_kind("220 mail.example.com ESMTP\n"), None);
    }

    #[test]
    fn probes_each_tcp_port_once_and_never_ssh() {
        let targets = probe_targets(&parse_ports(SS_ROOT));
        let ports: Vec<u16> = targets.iter().map(|t| t.port).collect();
        // SS_ROOT lists 22 twice, 8082 twice, 9090 once, plus UDP rows.
        assert_eq!(ports, vec![8082, 9090]);
    }

    #[test]
    fn probe_stays_inside_the_ssh_exec_timeout() {
        // The cap is not cosmetic: exceed it and the whole tab dies on the
        // 30s exec timeout instead of returning what it did find.
        assert!(MAX_PROBES as u32 * PROBE_TIMEOUT_SECS < ssh::EXEC_TIMEOUT.as_secs() as u32);
    }

    #[test]
    fn a_wildcard_bind_is_probed_over_loopback() {
        assert_eq!(probe_address("0.0.0.0"), "127.0.0.1");
        assert_eq!(probe_address("*"), "127.0.0.1");
        assert_eq!(probe_address("[::]"), "127.0.0.1");
        // A specific bind is dialled as it is -- loopback would not reach it.
        assert_eq!(probe_address("192.168.1.105"), "192.168.1.105");
    }

    #[test]
    fn the_probe_command_is_one_loop_over_every_target() {
        let cmd = probe_command(&probe_targets(&parse_ports(SS_ROOT)));
        assert!(cmd.contains("for hp in 127.0.0.1:8082 127.0.0.1:9090;"));
        assert!(cmd.contains("http://$hp/?action=stream"));
        assert!(cmd.contains("-m 2"), "each probe is time-boxed");
    }

    #[test]
    fn a_missing_tool_is_detected_from_exit_127() {
        let out = ExecOutput {
            stdout: String::new(),
            stderr: "sh: 1: ss: not found".into(),
            exit_status: 127,
        };
        assert!(tool_missing(&out));
        let unhappy = ExecOutput {
            stdout: String::new(),
            stderr: "Failed to list units: Connection refused".into(),
            exit_status: 1,
        };
        assert!(!tool_missing(&unhappy));
    }
}
