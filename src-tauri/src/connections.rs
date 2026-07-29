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

/// One saved connection = one cluster (a single-node install is a cluster of one).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    /// e.g. `https://pve.example.com:8006`
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
    fn unknown_profile_names_the_id() {
        let msg = map_err(ConnManagerError::UnknownProfile("pve-1".into()));
        assert!(msg.contains("pve-1"), "should name the missing id: {msg}");
    }
}
