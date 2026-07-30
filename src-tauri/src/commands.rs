//! Tauri commands — thin glue between the frontend and connections/proxmox.
//! Errors cross the bridge as strings; tokens never appear in return values.

use crate::connections::{self, ConnectionInfo};
use crate::proxmox::types::{
    AccessDomain, AccessRole, AccessUser, AclEntry, AcmeAccountEntry, BackupJob, CephDaemonAction,
    CephPool, CephServiceKind, CertificateInfo, ClusterResource, FirewallRule, GuestKind, HaGroup,
    HaResource, HaStatus, NodeNetwork, Permissions, PowerAction, ReplicationJob, StorageConfig,
    StorageContent, StorageSummary, TaskEntry, TaskLogLine, TaskStatus, Version,
};
use crate::proxmox::Client;
use crate::scan::{self, DiscoveredHost, TailscalePeer};

#[tauri::command]
pub fn list_connections(app: tauri::AppHandle) -> Result<Vec<ConnectionInfo>, String> {
    connections::load(&app)
}

#[tauri::command]
pub fn save_connection(
    app: tauri::AppHandle,
    info: ConnectionInfo,
    token: Option<String>,
    ssh_secret: Option<String>,
) -> Result<(), String> {
    let id = info.id.clone();
    connections::save(&app, info, token)?;
    if let Some(secret) = ssh_secret {
        connections::save_ssh_secret(&app, &id, &secret)?;
    }
    Ok(())
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

/// Network interfaces on a node, plus the pending-changes diff if any edits
/// are staged but not yet applied.
#[tauri::command]
pub async fn node_network(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<NodeNetwork, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.node_network(&node).await.map_err(|e| e.to_string())
}

/// Stage a new interface (bridge, bond, VLAN). Takes effect on apply.
#[tauri::command]
pub async fn create_network_iface(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .create_network_iface(&node, &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Replace an interface's definition. `params` must be the full definition —
/// omitted keys are dropped from the node's config.
#[tauri::command]
pub async fn update_network_iface(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    iface: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .update_network_iface(&node, &iface, &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Stage removal of an interface. Takes effect on apply.
#[tauri::command]
pub async fn delete_network_iface(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    iface: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_network_iface(&node, &iface)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Apply the staged network config (`ifreload -a`). Returns the task UPID.
/// A bad staged config can take the node's management link down with it.
#[tauri::command]
pub async fn apply_network(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.apply_network(&node).await.map_err(|e| e.to_string())
}

/// Discard the staged network config, leaving the running one alone.
#[tauri::command]
pub async fn revert_network(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .revert_network(&node)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
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

/// Back up guests now via vzdump. Returns the task UPID.
#[tauri::command]
pub async fn vzdump(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    params: std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .vzdump(&node, &params)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a storage volume (e.g. a backup archive).
#[tauri::command]
pub async fn delete_volume(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    storage: String,
    volid: String,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_volume(&node, &storage, &volid)
        .await
        .map_err(|e| e.to_string())
}

/// Scheduled backup jobs, cluster-wide.
#[tauri::command]
pub async fn backup_jobs(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<BackupJob>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.backup_jobs().await.map_err(|e| e.to_string())
}

/// Replication jobs, cluster-wide.
#[tauri::command]
pub async fn replication_jobs(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<ReplicationJob>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.replication_jobs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn access_users(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<AccessUser>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.access_users().await.map_err(|e| e.to_string())
}

/// Create a user (userid like name@pve; password only works for pve realm).
#[tauri::command]
pub async fn add_user(
    app: tauri::AppHandle,
    connection_id: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .add_user(&params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_user(
    app: tauri::AppHandle,
    connection_id: String,
    userid: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_user(&userid)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn access_domains(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<AccessDomain>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.access_domains().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn access_roles(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<AccessRole>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.access_roles().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn access_acl(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<AclEntry>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.access_acl().await.map_err(|e| e.to_string())
}

/// The connection's own effective privileges. Needs no privilege itself, so
/// this answers "would this action be refused?" even for a token that may not
/// read `/access/acl` — which is exactly the token the answer matters for.
#[tauri::command]
pub async fn access_permissions(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Permissions, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.access_permissions().await.map_err(|e| e.to_string())
}

/// Grant or revoke ACLs (path, roles, users; delete=1 revokes).
#[tauri::command]
pub async fn set_acl(
    app: tauri::AppHandle,
    connection_id: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .set_acl(&params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Cluster-wide storage definitions (storage.cfg).
#[tauri::command]
pub async fn storage_configs(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<StorageConfig>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.storage_configs().await.map_err(|e| e.to_string())
}

/// Add a storage definition.
#[tauri::command]
pub async fn add_storage(
    app: tauri::AppHandle,
    connection_id: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .add_storage(&params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Remove a storage definition (data on it is left untouched).
#[tauri::command]
pub async fn delete_storage(
    app: tauri::AppHandle,
    connection_id: String,
    storage: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_storage(&storage)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Guests currently under HA management, cluster-wide.
#[tauri::command]
pub async fn ha_resources(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<HaResource>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ha_resources().await.map_err(|e| e.to_string())
}

/// Put a guest under HA (params: sid like "qemu:100", state, group, ...).
#[tauri::command]
pub async fn add_ha_resource(
    app: tauri::AppHandle,
    connection_id: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .add_ha_resource(&params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_ha_resource(
    app: tauri::AppHandle,
    connection_id: String,
    sid: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .update_ha_resource(&sid, &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Take a guest out of HA. The guest itself is left alone.
#[tauri::command]
pub async fn delete_ha_resource(
    app: tauri::AppHandle,
    connection_id: String,
    sid: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_ha_resource(&sid)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ha_groups(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<HaGroup>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ha_groups().await.map_err(|e| e.to_string())
}

/// Create a failover group (params: group, nodes, restricted, nofailback).
#[tauri::command]
pub async fn add_ha_group(
    app: tauri::AppHandle,
    connection_id: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .add_ha_group(&params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_ha_group(
    app: tauri::AppHandle,
    connection_id: String,
    group: String,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .update_ha_group(&group, &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_ha_group(
    app: tauri::AppHandle,
    connection_id: String,
    group: String,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_ha_group(&group)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Live HA state. Clusters without HA answer with an empty list or an error —
/// callers treat both as "this cluster has no HA".
#[tauri::command]
pub async fn ha_status_current(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<HaStatus>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ha_status_current().await.map_err(|e| e.to_string())
}

/// Ceph health, mon quorum, PG states and capacity. Errors on a node with no
/// Ceph — the frontend uses that as its "is there Ceph here" probe.
#[tauri::command]
pub async fn ceph_status(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ceph_status(&node).await.map_err(|e| e.to_string())
}

/// The CRUSH tree. Flattened into a table by the frontend.
#[tauri::command]
pub async fn ceph_osds(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ceph_osds(&node).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ceph_pools(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<Vec<CephPool>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.ceph_pools(&node).await.map_err(|e| e.to_string())
}

/// MON, MGR or MDS listing, picked by `kind`.
#[tauri::command]
pub async fn ceph_services(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: CephServiceKind,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_services(&node, kind)
        .await
        .map_err(|e| e.to_string())
}

/// Mark an OSD in or out of the cluster.
#[tauri::command]
pub async fn ceph_osd_in_out(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    osdid: u32,
    into: bool,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    let res = if into {
        client.ceph_osd_in(&node, osdid).await
    } else {
        client.ceph_osd_out(&node, osdid).await
    };
    res.map_err(|e| e.to_string())
}

/// Start or stop an OSD's daemon.
#[tauri::command]
pub async fn ceph_osd_power(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    osdid: u32,
    action: CephDaemonAction,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_osd_power(&node, osdid, action)
        .await
        .map_err(|e| e.to_string())
}

/// Destroy an OSD. `cleanup` wipes the underlying disk as well.
#[tauri::command]
pub async fn ceph_osd_destroy(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    osdid: u32,
    cleanup: bool,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_osd_destroy(&node, osdid, cleanup)
        .await
        .map_err(|e| e.to_string())
}

/// Create a pool (params: name, size, min_size, pg_num, ...).
#[tauri::command]
pub async fn ceph_pool_create(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    params: std::collections::HashMap<String, String>,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_pool_create(&node, &params)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ceph_pool_update(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    name: String,
    params: std::collections::HashMap<String, String>,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_pool_update(&node, &name, &params)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a pool and its data. `remove_storages` also drops the PVE storage
/// entries backed by it.
#[tauri::command]
pub async fn ceph_pool_delete(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    name: String,
    remove_storages: bool,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .ceph_pool_delete(&node, &name, remove_storages)
        .await
        .map_err(|e| e.to_string())
}

/// The certificates pveproxy serves for this node.
#[tauri::command]
pub async fn certificates_info(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<Vec<CertificateInfo>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .certificates_info(&node)
        .await
        .map_err(|e| e.to_string())
}

/// Install a custom certificate (params: certificates, key, force, restart).
/// `params` carries the PEM private key: it is passed straight through and
/// must never be logged or echoed back in an error.
#[tauri::command]
pub async fn upload_certificate(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    params: std::collections::HashMap<String, String>,
) -> Result<CertificateInfo, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .upload_certificate(&node, &params)
        .await
        .map_err(|e| e.to_string())
}

/// Revert the node to its self-signed certificate.
#[tauri::command]
pub async fn delete_custom_certificate(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    restart: bool,
) -> Result<Option<String>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_custom_certificate(&node, restart)
        .await
        .map_err(|e| e.to_string())
}

/// Order the node's ACME certificate. Returns a task UPID.
#[tauri::command]
pub async fn acme_order_certificate(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .acme_order_certificate(&node)
        .await
        .map_err(|e| e.to_string())
}

/// Renew the node's ACME certificate. Returns a task UPID.
#[tauri::command]
pub async fn acme_renew_certificate(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    force: bool,
) -> Result<String, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .acme_renew_certificate(&node, force)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn acme_accounts(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<Vec<AcmeAccountEntry>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.acme_accounts().await.map_err(|e| e.to_string())
}

/// One ACME account's registration detail.
#[tauri::command]
pub async fn acme_account(
    app: tauri::AppHandle,
    connection_id: String,
    name: String,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.acme_account(&name).await.map_err(|e| e.to_string())
}

/// Configured ACME challenge plugins, read-only.
#[tauri::command]
pub async fn acme_plugins(
    app: tauri::AppHandle,
    connection_id: String,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client.acme_plugins().await.map_err(|e| e.to_string())
}

/// Firewall scope -> API path base. Cluster when node is None,
/// node when only node is set, guest when kind+vmid are set too.
fn fw_base(node: Option<String>, kind: Option<GuestKind>, vmid: Option<u32>) -> String {
    match (node, kind, vmid) {
        (Some(n), Some(k), Some(v)) => format!("/nodes/{n}/{}/{v}", k.as_path()),
        (Some(n), _, _) => format!("/nodes/{n}"),
        _ => "/cluster".to_string(),
    }
}

#[tauri::command]
pub async fn firewall_rules(
    app: tauri::AppHandle,
    connection_id: String,
    node: Option<String>,
    kind: Option<GuestKind>,
    vmid: Option<u32>,
) -> Result<Vec<FirewallRule>, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .firewall_rules(&fw_base(node, kind, vmid))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_firewall_rule(
    app: tauri::AppHandle,
    connection_id: String,
    node: Option<String>,
    kind: Option<GuestKind>,
    vmid: Option<u32>,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .add_firewall_rule(&fw_base(node, kind, vmid), &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_firewall_rule(
    app: tauri::AppHandle,
    connection_id: String,
    node: Option<String>,
    kind: Option<GuestKind>,
    vmid: Option<u32>,
    pos: u32,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .delete_firewall_rule(&fw_base(node, kind, vmid), pos)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Raw firewall options for a scope (enable, policy_in, ...).
#[tauri::command]
pub async fn firewall_options(
    app: tauri::AppHandle,
    connection_id: String,
    node: Option<String>,
    kind: Option<GuestKind>,
    vmid: Option<u32>,
) -> Result<serde_json::Value, String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .firewall_options(&fw_base(node, kind, vmid))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_firewall_options(
    app: tauri::AppHandle,
    connection_id: String,
    node: Option<String>,
    kind: Option<GuestKind>,
    vmid: Option<u32>,
    params: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client = connections::client_for(&app, &connection_id)?;
    client
        .set_firewall_options(&fw_base(node, kind, vmid), &params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Probe host+token before saving. For a saved connection pass no token —
/// it is read from the keyring.
#[tauri::command]
pub async fn test_connection(
    app: tauri::AppHandle,
    host: String,
    token: Option<String>,
    accept_invalid_certs: bool,
    connection_id: Option<String>,
) -> Result<Version, String> {
    let token = match (token, connection_id) {
        (Some(t), _) => t,
        (None, Some(id)) => connections::get_token(&app, &id)?,
        (None, None) => return Err("token or connectionId required".into()),
    };
    let client = Client::new(&host, &token, accept_invalid_certs).map_err(|e| e.to_string())?;
    client.version().await.map_err(|e| e.to_string())
}

/// Probe the local subnet(s) for hosts answering on the PVE web UI port.
#[tauri::command]
pub async fn scan_lan() -> Result<Vec<DiscoveredHost>, String> {
    scan::scan_lan().await
}

/// List the current tailnet's peers via `tailscale status --json`.
#[tauri::command]
pub async fn scan_tailscale() -> Result<Vec<TailscalePeer>, String> {
    scan::scan_tailscale().await
}
