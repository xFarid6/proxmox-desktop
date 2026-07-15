//! Tauri commands — thin glue between the frontend and connections/proxmox.
//! Errors cross the bridge as strings; tokens never appear in return values.

use crate::connections::{self, ConnectionInfo};
use crate::proxmox::types::{ClusterResource, GuestKind, PowerAction, Version};
use crate::proxmox::Client;

#[tauri::command]
pub fn list_connections(app: tauri::AppHandle) -> Result<Vec<ConnectionInfo>, String> {
    connections::load(&app)
}

#[tauri::command]
pub fn save_connection(
    app: tauri::AppHandle,
    info: ConnectionInfo,
    token: Option<String>,
) -> Result<(), String> {
    connections::save(&app, info, token)
}

#[tauri::command]
pub fn delete_connection(app: tauri::AppHandle, id: String) -> Result<(), String> {
    connections::delete(&app, &id)
}

/// The whole cluster in one call: nodes, guests, storage. A single-node
/// install is a cluster of one — same shape, N = 1.
#[tauri::command]
pub async fn cluster_resources(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<ClusterResource>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.cluster_resources().await.map_err(|e| e.to_string())
}

/// Start/stop/reboot/shutdown a guest. Returns the task UPID.
#[tauri::command]
pub async fn guest_power(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    vmid: u32,
    action: PowerAction,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .power(&node, kind, vmid, action)
        .await
        .map_err(|e| e.to_string())
}

/// Probe host+token before saving. For a saved connection pass no token —
/// it is read from the keyring.
#[tauri::command]
pub async fn test_connection(
    host: String,
    token: Option<String>,
    accept_invalid_certs: bool,
    connection_id: Option<String>,
) -> Result<Version, String> {
    let token = match (token, connection_id) {
        (Some(t), _) => t,
        (None, Some(id)) => connections::get_token(&id)?,
        (None, None) => return Err("token or connectionId required".into()),
    };
    let client = Client::new(&host, &token, accept_invalid_certs).map_err(|e| e.to_string())?;
    client.version().await.map_err(|e| e.to_string())
}
