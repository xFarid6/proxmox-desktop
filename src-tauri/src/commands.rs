//! Tauri commands — thin glue between the frontend and connections/proxmox.
//! Errors cross the bridge as strings; tokens never appear in return values.

use crate::connections::{self, ConnectionInfo};
use crate::proxmox::types::{
    ClusterResource, GuestKind, NetworkInterface, PowerAction, StorageContent, StorageSummary,
    TaskEntry, TaskLogLine, TaskStatus, Version,
};
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

/// Storages available on a node.
#[tauri::command]
pub async fn node_storages(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<Vec<StorageSummary>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.node_storages(&node).await.map_err(|e| e.to_string())
}

/// Volumes on a storage, optionally filtered by content type (iso, vztmpl, ...).
#[tauri::command]
pub async fn storage_content(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    storage: String,
    content: Option<String>,
) -> Result<Vec<StorageContent>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .storage_content(&node, &storage, content.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Create a VM or CT from raw params. Returns the creation task UPID.
#[tauri::command]
pub async fn create_guest(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    params: std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .create_guest(&node, kind, &params)
        .await
        .map_err(|e| e.to_string())
}

/// Raw guest config as JSON — key set varies wildly between qemu and lxc.
#[tauri::command]
pub async fn guest_config(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    vmid: u32,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .guest_config(&node, kind, vmid)
        .await
        .map_err(|e| e.to_string())
}

/// Update config fields (cores, memory, ...). Returns a UPID for qemu, None for lxc.
#[tauri::command]
pub async fn set_guest_config(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    vmid: u32,
    params: std::collections::HashMap<String, String>,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .set_guest_config(&node, kind, vmid, &params)
        .await
        .map_err(|e| e.to_string())
}

/// Grow a disk: size like "+5G". Shrinking is not supported by Proxmox.
#[tauri::command]
pub async fn resize_disk(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    vmid: u32,
    disk: String,
    size: String,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .resize_disk(&node, kind, vmid, &disk, &size)
        .await
        .map_err(|e| e.to_string())
}

/// Network interfaces on a node — read-only view.
#[tauri::command]
pub async fn node_network(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<Vec<NetworkInterface>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.node_network(&node).await.map_err(|e| e.to_string())
}

/// Recent tasks on a node (server-side limit 50).
#[tauri::command]
pub async fn node_tasks(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<Vec<TaskEntry>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.node_tasks(&node).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_status(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    upid: String,
) -> Result<TaskStatus, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .task_status(&node, &upid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_log(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    upid: String,
    start: Option<u64>,
) -> Result<Vec<TaskLogLine>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .task_log(&node, &upid, start.unwrap_or(0))
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
