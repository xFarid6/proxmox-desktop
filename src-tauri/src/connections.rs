//! Saved Proxmox connections. Host/name/cert-flag live in a JSON file in the
//! app config dir; the API token lives only in the OS keyring (Windows
//! Credential Manager / macOS Keychain / Secret Service) — never on disk,
//! never logged.
//!
//! Backed by the shared `conn-manager` crate's `ProfileStore`. The only
//! platform split left is which `SecretStore` it's built with: `OsKeyring`
//! (crate-provided) on desktop, `AndroidKeystore` (below, wrapping
//! `android_keystore.rs`) on Android — decided once, in `store()`.

#[cfg(not(target_os = "android"))]
use conn_manager::OsKeyring;
use conn_manager::{secondary_key, ConnManagerError, Profile, ProfileStore};
#[cfg(target_os = "android")]
use conn_manager::{SecretError, SecretStore};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::proxmox::Client;

const KEYRING_SERVICE: &str = "proxmox-desktop";

/// Name the SSH secret (password, or key passphrase) rides under in the
/// keyring, alongside the profile's own API-token secret. `conn-manager`'s
/// `ProfileStore` keys one secret directly by profile id; a second secret
/// for the same profile has to be namespaced or it would collide with the
/// token, so this is stored under `secondary_key(id, "ssh")` instead of
/// reaching for `keyring` directly (which would bypass whichever
/// `SecretStore` this app was built with, breaking Android).
const SSH_SECRET_NAME: &str = "ssh";

/// Whether a saved connection is a Proxmox cluster or a plain SSH host.
///
/// `#[serde(default)]` on the field below is load-bearing: every profile
/// already written to a user's disk predates this field, and must keep
/// deserialising as `Pve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    #[default]
    Pve,
    Ssh,
}

/// One saved connection = one cluster (a single-node install is a cluster of one).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: ConnectionKind,
    /// e.g. `https://pve.example.com:8006`. For `ConnectionKind::Ssh` this is
    /// a bare hostname or IP -- no scheme, no port.
    pub host: String,
    /// Explicit per-connection opt-in for self-signed certs.
    pub accept_invalid_certs: bool,
    /// SSH shell config, if this connection has one set up. The secret
    /// (password or key passphrase) lives in the keyring under
    /// `SSH_SECRET_NAME`, never here.
    pub ssh: Option<SshInfo>,
}

impl Profile for ConnectionInfo {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Per-connection SSH shell config. Auth method is picked by which fields
/// are set: `key_path` means key-file auth (the secret is the key's
/// passphrase, empty if it's not encrypted); otherwise `use_agent` tries
/// the platform's running ssh-agent/Pageant; otherwise the secret is a
/// plain password.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshInfo {
    pub user: String,
    pub port: u16,
    pub key_path: Option<String>,
    pub use_agent: bool,
}

/// Routes secret storage to the Kotlin `KeystorePlugin` via
/// `android_keystore.rs`. `service` is ignored — the plugin is already
/// scoped to this app.
#[cfg(target_os = "android")]
struct AndroidKeystore(tauri::AppHandle);

#[cfg(target_os = "android")]
impl SecretStore for AndroidKeystore {
    fn get(&self, _service: &str, id: &str) -> Result<String, SecretError> {
        crate::android_keystore::get(&self.0, id).map_err(SecretError::Other)
    }

    fn set(&self, _service: &str, id: &str, secret: &str) -> Result<(), SecretError> {
        crate::android_keystore::set(&self.0, id, secret).map_err(SecretError::Other)
    }

    fn delete(&self, _service: &str, id: &str) {
        let _ = crate::android_keystore::delete(&self.0, id);
    }
}

#[cfg(not(target_os = "android"))]
type Store = ProfileStore<OsKeyring>;
#[cfg(target_os = "android")]
type Store = ProfileStore<AndroidKeystore>;

/// The one `cfg` split in this file. Everything below is platform-agnostic.
fn store(app: &tauri::AppHandle) -> Result<Store, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "android"))]
    let store = ProfileStore::new(dir, KEYRING_SERVICE);
    #[cfg(target_os = "android")]
    let store = ProfileStore::with_secret_store(dir, KEYRING_SERVICE, AndroidKeystore(app.clone()));
    Ok(store)
}

/// `ConnManagerError` at the command boundary. Matched on the variant rather
/// than passed through `Display` — `Io` embeds the raw `io::Error` message
/// (e.g. `(os error 5)`), which isn't something to show a user.
fn map_err(e: ConnManagerError) -> String {
    match e {
        ConnManagerError::Io(_) => "failed to read or write the connections file".into(),
        ConnManagerError::Serde(_) => "the connections file is corrupted".into(),
        ConnManagerError::Secret(e) => e.to_string(),
        ConnManagerError::UnknownProfile(id) => format!("unknown connection: {id}"),
    }
}

pub fn load(app: &tauri::AppHandle) -> Result<Vec<ConnectionInfo>, String> {
    store(app)?.load().map_err(map_err)
}

pub fn get_token(app: &tauri::AppHandle, id: &str) -> Result<String, String> {
    store(app)?.get_secret(id).map_err(map_err)
}

/// Info from disk + token from keyring for a saved connection.
pub fn info_and_token(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<(ConnectionInfo, String), String> {
    let store = store(app)?;
    let info = store.get(id).map_err(map_err)?;
    let token = store.get_secret(id).map_err(map_err)?;
    Ok((info, token))
}

/// Info + SSH config for a saved connection that has SSH set up. The
/// secret (password/passphrase) is read separately via `get_ssh_secret` --
/// agent auth needs no secret at all, so callers ask for it only when the
/// config actually calls for one.
pub fn info_and_ssh(app: &tauri::AppHandle, id: &str) -> Result<(ConnectionInfo, SshInfo), String> {
    let info = store(app)?.get::<ConnectionInfo>(id).map_err(map_err)?;
    let ssh = info
        .ssh
        .clone()
        .ok_or_else(|| "no SSH credentials configured for this connection".to_string())?;
    Ok((info, ssh))
}

/// Everything needed to open an SSH connection to a saved connection's host,
/// with the secret owned here so `auth()` can hand out a borrowing
/// `ssh::Auth` without the caller juggling two lifetimes.
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    key_path: Option<String>,
    use_agent: bool,
    secret: String,
}

impl SshTarget {
    /// Which of the three auth methods this connection's config selects.
    /// Mirrors the field precedence documented on `SshInfo`.
    pub fn auth(&self) -> crate::ssh::Auth<'_> {
        if self.use_agent {
            crate::ssh::Auth::Agent
        } else if let Some(path) = &self.key_path {
            crate::ssh::Auth::Key {
                path: std::path::Path::new(path),
                passphrase: &self.secret,
            }
        } else {
            crate::ssh::Auth::Password(&self.secret)
        }
    }
}

/// Strips scheme and port off a Proxmox API host (`https://pve.example.com:8006`
/// -> `pve.example.com`) to get the SSH target. proxmox-desktop stores one
/// host per connection, so this assumes the SSH endpoint lives on the same
/// machine as the API -- true for the common single-node case; a genuine
/// multi-node cluster with per-node SSH endpoints isn't modeled yet.
fn ssh_host(api_host: &str) -> String {
    let without_scheme = api_host.rsplit("://").next().unwrap_or(api_host);
    without_scheme
        .split(':')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Resolve a saved connection into a ready-to-dial SSH target: host from the
/// API URL, config from disk, secret from the keyring. Agent auth reads no
/// secret at all, so a connection set up for the agent works even with
/// nothing stored under its SSH key.
pub fn ssh_target(app: &tauri::AppHandle, id: &str) -> Result<SshTarget, String> {
    let (info, ssh) = info_and_ssh(app, id)?;
    let secret = if ssh.use_agent {
        String::new()
    } else {
        get_ssh_secret(app, id)?
    };
    Ok(SshTarget {
        host: ssh_host(&info.host),
        port: ssh.port,
        user: ssh.user,
        key_path: ssh.key_path,
        use_agent: ssh.use_agent,
        secret,
    })
}

pub fn get_ssh_secret(app: &tauri::AppHandle, id: &str) -> Result<String, String> {
    store(app)?
        .get_secret(&secondary_key(id, SSH_SECRET_NAME))
        .map_err(map_err)
}

pub fn save_ssh_secret(app: &tauri::AppHandle, id: &str, secret: &str) -> Result<(), String> {
    store(app)?
        .set_secret(&secondary_key(id, SSH_SECRET_NAME), secret)
        .map_err(map_err)
}

/// Build an API client for a saved connection (info from disk, token from keyring).
pub fn client_for(app: &tauri::AppHandle, id: &str) -> Result<Client, String> {
    let (info, token) = info_and_token(app, id)?;
    Client::new(&info.host, &token, info.accept_invalid_certs).map_err(|e| e.to_string())
}

/// Upsert a connection; `token` is written to the keyring when provided
/// (add, or edit that changes the token).
pub fn save(
    app: &tauri::AppHandle,
    info: ConnectionInfo,
    token: Option<String>,
) -> Result<(), String> {
    store(app)?.save(info, token).map_err(map_err)
}

pub fn delete(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let store = store(app)?;
    // `delete` below only removes the secret keyed by the profile id (the
    // API token) -- the SSH secret rides a namespaced key of its own, so it
    // would otherwise be orphaned in the keyring after this connection is
    // gone.
    store.delete_secret(&secondary_key(id, SSH_SECRET_NAME));
    store.delete::<ConnectionInfo>(id).map_err(map_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    /// `map_err` exists to keep raw OS error text out of the UI. `io::Error`'s
    /// `Display` embeds strings like "(os error 5)" on Windows and Unix alike,
    /// so assert the mapped message carries none of it — a future edit that
    /// "adds the detail back" should fail here.
    #[test]
    fn io_and_serde_errors_do_not_leak_the_underlying_message() {
        let io = ConnManagerError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "os error 5",
        ));
        let msg = map_err(io);
        assert!(!msg.contains("os error"), "leaked OS error text: {msg}");

        let serde =
            ConnManagerError::Serde(serde_json::from_str::<ConnectionInfo>("{").unwrap_err());
        let msg = map_err(serde);
        assert!(!msg.contains("line"), "leaked serde position detail: {msg}");
    }

    #[test]
    fn ssh_host_strips_scheme_and_port() {
        assert_eq!(ssh_host("https://pve.example.com:8006"), "pve.example.com");
        assert_eq!(ssh_host("http://10.0.0.5:8006"), "10.0.0.5");
        assert_eq!(ssh_host("pve.example.com"), "pve.example.com");
    }

    /// The three auth methods are selected by field precedence, not by an
    /// explicit tag, so assert the precedence rather than trusting it.
    #[test]
    fn auth_precedence_is_agent_then_key_then_password() {
        let base = SshTarget {
            host: "h".into(),
            port: 22,
            user: "root".into(),
            key_path: None,
            use_agent: false,
            secret: "s3cret".into(),
        };

        let agent = SshTarget {
            use_agent: true,
            key_path: Some("/k".into()),
            ..base_clone(&base)
        };
        assert!(matches!(agent.auth(), crate::ssh::Auth::Agent));

        let key = SshTarget {
            key_path: Some("/k".into()),
            ..base_clone(&base)
        };
        assert!(matches!(key.auth(), crate::ssh::Auth::Key { .. }));

        assert!(matches!(base.auth(), crate::ssh::Auth::Password("s3cret")));
    }

    fn base_clone(t: &SshTarget) -> SshTarget {
        SshTarget {
            host: t.host.clone(),
            port: t.port,
            user: t.user.clone(),
            key_path: t.key_path.clone(),
            use_agent: t.use_agent,
            secret: t.secret.clone(),
        }
    }

    #[test]
    fn unknown_profile_names_the_id() {
        let msg = map_err(ConnManagerError::UnknownProfile("pve-1".into()));
        assert!(msg.contains("pve-1"), "should name the missing id: {msg}");
    }

    /// Every profile already on a user's disk predates the `kind` field.
    /// Deserialising one without a `kind` key must silently become `Pve`,
    /// not fail to load or default to something else -- this is the test
    /// that catches silently breaking every existing install.
    #[test]
    fn a_profile_saved_before_the_kind_field_loads_as_pve() {
        let json = r#"{"id":"pve-1","name":"home","host":"https://pve.example.com:8006","acceptInvalidCerts":false}"#;
        let info: ConnectionInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.kind, ConnectionKind::Pve);
    }

    /// The frontend's TS union is the lowercase strings `"pve"` / `"ssh"`,
    /// so the casing is a contract, not a detail.
    #[test]
    fn the_kind_field_round_trips_as_a_lowercase_string() {
        let info = ConnectionInfo {
            id: "ssh-1".into(),
            name: "wyse-server".into(),
            kind: ConnectionKind::Ssh,
            host: "wyse-server".into(),
            accept_invalid_certs: false,
            ssh: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""kind":"ssh""#), "{json}");
    }
}
