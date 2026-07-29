//! Network discovery: find hosts worth connecting to without an nmap
//! dependency. Two independent sources:
//!
//! - `scan_lan`: probe the local subnet(s) on the Proxmox web UI port
//!   (8006), then confirm each open port really is a PVE API via its
//!   unauthenticated `/api2/json/version` endpoint.
//! - `scan_tailscale`: shell out to `tailscale status --json` and list
//!   tailnet peers as connect candidates.

use serde::Serialize;
use std::net::Ipv4Addr;
use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const PVE_PORT: u16 = 8006;
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);
const CONCURRENCY: usize = 64;
/// Skip any interface whose subnet is bigger than a /22 (1024 addresses) —
/// a misconfigured netmask shouldn't turn this into a scan of the internet.
const MAX_HOSTS: usize = 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredHost {
    pub ip: String,
    /// Ready to paste into the add-connection form.
    pub host: String,
    /// Set only when `/api2/json/version` answered — `None` means the port
    /// was open but it didn't look like a PVE API.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscalePeer {
    pub name: String,
    pub ip: String,
    pub online: bool,
    pub os: String,
}

#[cfg(not(target_os = "android"))]
pub async fn scan_lan() -> Result<Vec<DiscoveredHost>, String> {
    let candidates = local_subnet_hosts().map_err(|e| e.to_string())?;
    let found = stream::iter(candidates)
        .map(probe)
        .buffer_unordered(CONCURRENCY)
        .filter_map(|h| async { h })
        .collect()
        .await;
    Ok(found)
}

#[cfg(target_os = "android")]
pub async fn scan_lan() -> Result<Vec<DiscoveredHost>, String> {
    Err("LAN scan isn't supported on Android".into())
}

#[cfg(not(target_os = "android"))]
fn local_subnet_hosts() -> std::io::Result<Vec<Ipv4Addr>> {
    let mut hosts = Vec::new();
    for iface in if_addrs::get_if_addrs()? {
        if iface.is_loopback() {
            continue;
        }
        if let if_addrs::IfAddr::V4(v4) = iface.addr {
            let subnet = hosts_in_subnet(v4.ip, v4.netmask);
            if subnet.len() <= MAX_HOSTS {
                hosts.extend(subnet);
            }
        }
    }
    Ok(hosts)
}

fn hosts_in_subnet(ip: Ipv4Addr, netmask: Ipv4Addr) -> Vec<Ipv4Addr> {
    let mask = u32::from(netmask);
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;
    // Exclude the network and broadcast addresses themselves.
    ((network + 1)..broadcast).map(Ipv4Addr::from).collect()
}

async fn probe(ip: Ipv4Addr) -> Option<DiscoveredHost> {
    let addr = std::net::SocketAddr::from((ip, PVE_PORT));
    timeout(PROBE_TIMEOUT, TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;
    let host = format!("https://{ip}:{PVE_PORT}");
    let version = probe_version(&host).await;
    Some(DiscoveredHost {
        ip: ip.to_string(),
        host,
        version,
    })
}

/// PVE's `/version` endpoint answers unauthenticated, so a plain GET
/// distinguishes "this is Proxmox" from "something else is on 8006".
async fn probe_version(host: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(VERSION_TIMEOUT)
        .build()
        .ok()?;
    let resp = client
        .get(format!("{host}/api2/json/version"))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("data")?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

/// Blocks briefly on a local CLI call (near-instant, no network wait) —
/// not worth pulling in tokio's `process` feature for.
pub async fn scan_tailscale() -> Result<Vec<TailscalePeer>, String> {
    let output = std::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .map_err(|e| format!("failed to run `tailscale status`: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let peers = json
        .get("Peer")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();

    let mut result: Vec<TailscalePeer> = peers
        .values()
        .filter_map(|p| {
            let name = p.get("HostName")?.as_str()?.to_string();
            let ip = p
                .get("TailscaleIPs")?
                .as_array()?
                .first()?
                .as_str()?
                .to_string();
            let online = p.get("Online").and_then(|o| o.as_bool()).unwrap_or(false);
            let os = p
                .get("OS")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string();
            Some(TailscalePeer {
                name,
                ip,
                online,
                os,
            })
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_excludes_network_and_broadcast() {
        let hosts = hosts_in_subnet(
            "192.168.1.10".parse().unwrap(),
            "255.255.255.0".parse().unwrap(),
        );
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], "192.168.1.1".parse::<Ipv4Addr>().unwrap());
        assert_eq!(
            hosts[hosts.len() - 1],
            "192.168.1.254".parse::<Ipv4Addr>().unwrap()
        );
    }

    #[test]
    fn tailscale_json_parses_peers() {
        let raw = serde_json::json!({
            "Peer": {
                "a": {"HostName": "wyse-server", "TailscaleIPs": ["100.77.208.85"], "Online": true, "OS": "linux"},
                "b": {"HostName": "phone", "TailscaleIPs": ["100.1.2.3"], "Online": false, "OS": "android"}
            }
        });
        let peers: Vec<TailscalePeer> = raw
            .get("Peer")
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .filter_map(|p| {
                let name = p.get("HostName")?.as_str()?.to_string();
                let ip = p
                    .get("TailscaleIPs")?
                    .as_array()?
                    .first()?
                    .as_str()?
                    .to_string();
                let online = p.get("Online").and_then(|o| o.as_bool()).unwrap_or(false);
                let os = p
                    .get("OS")
                    .and_then(|o| o.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(TailscalePeer {
                    name,
                    ip,
                    online,
                    os,
                })
            })
            .collect();
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().any(|p| p.name == "wyse-server" && p.online));
    }
}
