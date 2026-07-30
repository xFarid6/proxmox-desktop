//! `pxx` -- a headless CLI that drives the real Proxmox HTTP client
//! (`proxmox_desktop_lib::proxmox::Client`) with no GUI, no Tauri runtime,
//! and no click-through. Meant for exercising a real cluster from a
//! terminal (or a script, or an AI agent) the same way the desktop app's
//! backend does.
//!
//! Read-only by design: every subcommand maps to a `GET` on the Proxmox
//! API. Power, delete, create, vzdump and firewall-write actions are
//! deliberately left out -- this tool is for inspection, not for driving
//! mutations against a live cluster unattended.
//!
//! Credentials come from environment variables only, never CLI flags and
//! never a file on disk, so nothing sensitive ends up in shell history or
//! a config file:
//!
//!   PXX_HOST      e.g. `https://100.80.231.52:8006`
//!   PXX_TOKEN     full Proxmox API token: `user@realm!tokenid=uuid`
//!   PXX_INSECURE  set to `1` to accept a self-signed certificate
//!
//! `PXX_TOKEN` is read once into a `String`, handed straight to
//! `Client::new`, and never printed, logged, or included in any error
//! message produced by this binary.

use proxmox_desktop_lib::proxmox::types::GuestKind;
use proxmox_desktop_lib::proxmox::Client;
use proxmox_desktop_lib::scan;

fn usage() -> &'static str {
    "usage: pxx <command> [args]\n\
\n\
commands (all read-only, print pretty JSON to stdout):\n\
  version\n\
  resources\n\
  guest-config <node> <qemu|lxc> <vmid>\n\
  node-network <node>\n\
  storages <node>\n\
  tasks <node>\n\
  scan-lan          -- no credentials needed\n\
  scan-tailscale    -- no credentials needed\n\
\n\
env (credentials only -- never passed as flags):\n\
  PXX_HOST      e.g. https://100.80.231.52:8006\n\
  PXX_TOKEN     Proxmox API token, user@realm!tokenid=uuid\n\
  PXX_INSECURE  set to 1 to accept a self-signed certificate\n"
}

/// One parsed CLI invocation. Kept separate from execution so parsing can
/// be unit tested without spinning up a runtime or touching the network.
#[derive(Debug)]
enum Command {
    Version,
    Resources,
    GuestConfig {
        node: String,
        kind: GuestKind,
        vmid: u32,
    },
    NodeNetwork {
        node: String,
    },
    Storages {
        node: String,
    },
    Tasks {
        node: String,
    },
    ScanLan,
    ScanTailscale,
}

fn parse_guest_kind(s: &str) -> Result<GuestKind, String> {
    match s {
        "qemu" => Ok(GuestKind::Qemu),
        "lxc" => Ok(GuestKind::Lxc),
        other => Err(format!(
            "unknown guest kind '{other}' (expected 'qemu' or 'lxc')"
        )),
    }
}

/// Parses argv (already stripped of argv[0]) into a `Command`. Every
/// failure path returns `Err` with a message naming what was wrong --
/// nothing here panics or indexes out of bounds.
fn parse_args(args: &[String]) -> Result<Command, String> {
    let Some(cmd) = args.first() else {
        return Err(format!("missing subcommand\n\n{}", usage()));
    };
    match cmd.as_str() {
        "version" => Ok(Command::Version),
        "resources" => Ok(Command::Resources),
        "guest-config" => {
            let node = args.get(1).ok_or("guest-config: missing <node>")?.clone();
            let kind = parse_guest_kind(args.get(2).ok_or("guest-config: missing <qemu|lxc>")?)?;
            let vmid: u32 = args
                .get(3)
                .ok_or("guest-config: missing <vmid>")?
                .parse()
                .map_err(|_| "guest-config: <vmid> must be a positive integer".to_string())?;
            Ok(Command::GuestConfig { node, kind, vmid })
        }
        "node-network" => Ok(Command::NodeNetwork {
            node: args.get(1).ok_or("node-network: missing <node>")?.clone(),
        }),
        "storages" => Ok(Command::Storages {
            node: args.get(1).ok_or("storages: missing <node>")?.clone(),
        }),
        "tasks" => Ok(Command::Tasks {
            node: args.get(1).ok_or("tasks: missing <node>")?.clone(),
        }),
        "scan-lan" => Ok(Command::ScanLan),
        "scan-tailscale" => Ok(Command::ScanTailscale),
        other => Err(format!("unknown command '{other}'\n\n{}", usage())),
    }
}

/// Reads a required env var, naming it in the error if unset. Used for
/// `PXX_TOKEN` too -- only ever reports whether it is set, never its value.
fn require_env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is not set"))
}

fn build_client() -> Result<Client, String> {
    let host = require_env("PXX_HOST")?;
    let token = require_env("PXX_TOKEN")?;
    let insecure = std::env::var("PXX_INSECURE")
        .map(|v| v == "1")
        .unwrap_or(false);
    Client::new(&host, &token, insecure).map_err(|e| e.to_string())
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

async fn run(command: Command) -> Result<(), String> {
    match command {
        Command::ScanLan => print_json(&scan::scan_lan().await?),
        Command::ScanTailscale => print_json(&scan::scan_tailscale().await?),
        Command::Version => {
            print_json(&build_client()?.version().await.map_err(|e| e.to_string())?)
        }
        Command::Resources => print_json(
            &build_client()?
                .cluster_resources()
                .await
                .map_err(|e| e.to_string())?,
        ),
        Command::GuestConfig { node, kind, vmid } => print_json(
            &build_client()?
                .guest_config(&node, kind, vmid)
                .await
                .map_err(|e| e.to_string())?,
        ),
        Command::NodeNetwork { node } => print_json(
            &build_client()?
                .node_network(&node)
                .await
                .map_err(|e| e.to_string())?,
        ),
        Command::Storages { node } => print_json(
            &build_client()?
                .node_storages(&node)
                .await
                .map_err(|e| e.to_string())?,
        ),
        Command::Tasks { node } => print_json(
            &build_client()?
                .node_tasks(&node)
                .await
                .map_err(|e| e.to_string())?,
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start async runtime: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = rt.block_on(run(command)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Neither an unknown subcommand nor a missing required argument should
    /// ever panic -- both must come back as a plain `Err` with a message,
    /// since this is exactly the input an agent or a typo-prone human is
    /// most likely to produce first.
    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let unknown = parse_args(&["bogus".to_string()]);
        assert!(unknown.is_err());
        assert!(unknown.unwrap_err().contains("unknown command"));

        let missing_args = parse_args(&["guest-config".to_string()]);
        assert!(missing_args.is_err());
        assert!(missing_args.unwrap_err().contains("missing <node>"));
    }
}
