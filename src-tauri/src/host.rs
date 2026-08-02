//! Read-only facts about a plain SSH host: what is listening, and what
//! systemd is running (issue #104).
//!
//! Unlike [`crate::docker`], there is no guest hop here — an "SSH host"
//! connection (#102) *is* the machine, so every command is one
//! [`ssh::exec_on_connection`] call plus a parser:
//!
//! ```text
//! app --ssh--> host --> ss / systemctl
//! ```
//!
//! Both commands are pure reads. Starting, stopping or restarting a unit is
//! deliberately out of scope for #104.

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

fn parse_ports(stdout: &str) -> Vec<ListeningPort> {
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
    let out = ssh::exec_on_connection(&app, &sessions, &connection_id, "ss -tulnp").await?;
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
