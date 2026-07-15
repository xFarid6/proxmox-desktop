//! Saved Proxmox connections. Host/name/cert-flag live in a JSON file in the
//! app config dir; the API token lives only in the OS keyring (Windows
//! Credential Manager / macOS Keychain / Secret Service) — never on disk,
//! never logged.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

use crate::proxmox::Client;

const KEYRING_SERVICE: &str = "proxmox-desktop";

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
}

fn store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("connections.json"))
}

pub fn load(app: &tauri::AppHandle) -> Result<Vec<ConnectionInfo>, String> {
    let path = store_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save_all(app: &tauri::AppHandle, conns: &[ConnectionInfo]) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(conns).map_err(|e| e.to_string())?;
    fs::write(store_path(app)?, raw).map_err(|e| e.to_string())
}

fn token_entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|e| e.to_string())
}

pub fn get_token(id: &str) -> Result<String, String> {
    token_entry(id)?.get_password().map_err(|e| e.to_string())
}

/// Build an API client for a saved connection (info from disk, token from keyring).
pub fn client_for(app: &tauri::AppHandle, id: &str) -> Result<Client, String> {
    let conns = load(app)?;
    let info = conns
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("unknown connection: {id}"))?;
    let token = get_token(id)?;
    Client::new(&info.host, &token, info.accept_invalid_certs).map_err(|e| e.to_string())
}

/// Upsert a connection; `token` is written to the keyring when provided
/// (add, or edit that changes the token).
pub fn save(
    app: &tauri::AppHandle,
    info: ConnectionInfo,
    token: Option<String>,
) -> Result<(), String> {
    if let Some(t) = token {
        token_entry(&info.id)?
            .set_password(&t)
            .map_err(|e| e.to_string())?;
    }
    let mut conns = load(app)?;
    match conns.iter_mut().find(|c| c.id == info.id) {
        Some(existing) => *existing = info,
        None => conns.push(info),
    }
    save_all(app, &conns)
}

pub fn delete(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    if let Ok(entry) = token_entry(id) {
        // Best effort — the entry may already be gone.
        let _ = entry.delete_credential();
    }
    let mut conns = load(app)?;
    conns.retain(|c| c.id != id);
    save_all(app, &conns)
}
