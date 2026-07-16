pub mod error;
pub mod types;

use std::collections::HashMap;

pub use error::{Error, Result};
use serde::de::DeserializeOwned;
use types::*;

/// Typed client for the Proxmox VE HTTP API.
///
/// Auth is an API token sent as `Authorization: PVEAPIToken=user@realm!tokenid=uuid`.
/// Self-signed TLS certs are rejected unless the caller explicitly opts in
/// per connection. No Debug impl — the token must never end up in logs.
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    auth_header: String,
}

impl Client {
    /// `base_url` like `https://pve.example.com:8006`, `token` the full
    /// `user@realm!tokenid=uuid` value.
    pub fn new(base_url: &str, token: &str, accept_invalid_certs: bool) -> Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(accept_invalid_certs)
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header: format!("PVEAPIToken={token}"),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api2/json{}", self.base_url, path)
    }

    async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        let wrapped: ApiResponse<T> =
            serde_json::from_str(&body).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(wrapped.data)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .http
            .get(self.url(path))
            .header("Authorization", &self.auth_header)
            .send()
            .await?;
        Self::decode(resp).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        form: &HashMap<String, String>,
    ) -> Result<T> {
        let resp = self
            .http
            .post(self.url(path))
            .header("Authorization", &self.auth_header)
            .form(form)
            .send()
            .await?;
        Self::decode(resp).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        form: &HashMap<String, String>,
    ) -> Result<T> {
        let resp = self
            .http
            .put(self.url(path))
            .header("Authorization", &self.auth_header)
            .form(form)
            .send()
            .await?;
        Self::decode(resp).await
    }

    async fn delete_req<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .http
            .delete(self.url(path))
            .header("Authorization", &self.auth_header)
            .send()
            .await?;
        Self::decode(resp).await
    }

    /// Cheap auth + reachability probe.
    pub async fn version(&self) -> Result<Version> {
        self.get("/version").await
    }

    /// Everything in the cluster: nodes, guests, storage. One call powers
    /// dashboard and guest list. A single-node install is a cluster of one.
    pub async fn cluster_resources(&self) -> Result<Vec<ClusterResource>> {
        self.get("/cluster/resources").await
    }

    pub async fn nodes(&self) -> Result<Vec<NodeSummary>> {
        self.get("/nodes").await
    }

    pub async fn node_network(&self, node: &str) -> Result<Vec<NetworkInterface>> {
        self.get(&format!("/nodes/{node}/network")).await
    }

    pub async fn node_storages(&self, node: &str) -> Result<Vec<StorageSummary>> {
        self.get(&format!("/nodes/{node}/storage")).await
    }

    /// `content` filters e.g. "iso", "vztmpl", "images".
    pub async fn storage_content(
        &self,
        node: &str,
        storage: &str,
        content: Option<&str>,
    ) -> Result<Vec<StorageContent>> {
        let mut path = format!("/nodes/{node}/storage/{storage}/content");
        if let Some(c) = content {
            path.push_str(&format!("?content={c}"));
        }
        self.get(&path).await
    }

    /// Raw config map — keys vary per guest (net0, scsi0, cores, ...).
    pub async fn guest_config(
        &self,
        node: &str,
        kind: GuestKind,
        vmid: u32,
    ) -> Result<serde_json::Value> {
        self.get(&format!("/nodes/{node}/{}/{vmid}/config", kind.as_path()))
            .await
    }

    /// Update config fields (cores, memory, ...). Qemu uses async POST and
    /// returns a UPID; LXC uses sync PUT and returns null.
    pub async fn set_guest_config(
        &self,
        node: &str,
        kind: GuestKind,
        vmid: u32,
        params: &HashMap<String, String>,
    ) -> Result<Option<String>> {
        let path = format!("/nodes/{node}/{}/{vmid}/config", kind.as_path());
        match kind {
            GuestKind::Qemu => self.post(&path, params).await,
            GuestKind::Lxc => self.put(&path, params).await,
        }
    }

    /// `size` like "+5G" (grow by) or "50G" (absolute).
    pub async fn resize_disk(
        &self,
        node: &str,
        kind: GuestKind,
        vmid: u32,
        disk: &str,
        size: &str,
    ) -> Result<Option<String>> {
        let mut params = HashMap::new();
        params.insert("disk".to_string(), disk.to_string());
        params.insert("size".to_string(), size.to_string());
        self.put(
            &format!("/nodes/{node}/{}/{vmid}/resize", kind.as_path()),
            &params,
        )
        .await
    }

    /// Start/stop/reboot/shutdown. Returns the task UPID.
    pub async fn power(
        &self,
        node: &str,
        kind: GuestKind,
        vmid: u32,
        action: PowerAction,
    ) -> Result<String> {
        self.post(
            &format!(
                "/nodes/{node}/{}/{vmid}/status/{}",
                kind.as_path(),
                action.as_path()
            ),
            &HashMap::new(),
        )
        .await
    }

    /// Create a VM or CT. Caller supplies Proxmox form params
    /// (vmid, cores, memory, net0, ...). Returns the task UPID.
    pub async fn create_guest(
        &self,
        node: &str,
        kind: GuestKind,
        params: &HashMap<String, String>,
    ) -> Result<String> {
        self.post(&format!("/nodes/{node}/{}", kind.as_path()), params)
            .await
    }

    pub async fn node_tasks(&self, node: &str) -> Result<Vec<TaskEntry>> {
        self.get(&format!("/nodes/{node}/tasks?limit=50")).await
    }

    pub async fn task_status(&self, node: &str, upid: &str) -> Result<TaskStatus> {
        self.get(&format!("/nodes/{node}/tasks/{upid}/status"))
            .await
    }

    pub async fn task_log(&self, node: &str, upid: &str, start: u64) -> Result<Vec<TaskLogLine>> {
        self.get(&format!("/nodes/{node}/tasks/{upid}/log?start={start}"))
            .await
    }

    /// Back up guests now via vzdump (params: vmid, storage, mode, compress, ...).
    /// Returns the task UPID.
    pub async fn vzdump(&self, node: &str, params: &HashMap<String, String>) -> Result<String> {
        self.post(&format!("/nodes/{node}/vzdump"), params).await
    }

    /// Delete a volume (e.g. a backup archive). Returns a UPID or null
    /// depending on storage type.
    pub async fn delete_volume(
        &self,
        node: &str,
        storage: &str,
        volid: &str,
    ) -> Result<Option<String>> {
        self.delete_req(&format!("/nodes/{node}/storage/{storage}/content/{volid}"))
            .await
    }

    /// Scheduled backup jobs, cluster-wide.
    pub async fn backup_jobs(&self) -> Result<Vec<BackupJob>> {
        self.get("/cluster/backup").await
    }

    /// Replication jobs, cluster-wide.
    pub async fn replication_jobs(&self) -> Result<Vec<ReplicationJob>> {
        self.get("/cluster/replication").await
    }

    /// Firewall endpoints share one shape across scopes; `base` is
    /// "/cluster", "/nodes/{node}" or "/nodes/{node}/{qemu|lxc}/{vmid}".
    pub async fn firewall_rules(&self, base: &str) -> Result<Vec<FirewallRule>> {
        self.get(&format!("{base}/firewall/rules")).await
    }

    pub async fn add_firewall_rule(
        &self,
        base: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.post(&format!("{base}/firewall/rules"), params).await
    }

    pub async fn delete_firewall_rule(&self, base: &str, pos: u32) -> Result<serde_json::Value> {
        self.delete_req(&format!("{base}/firewall/rules/{pos}"))
            .await
    }

    /// Raw options map — key set differs per scope (enable, policy_in, ...).
    pub async fn firewall_options(&self, base: &str) -> Result<serde_json::Value> {
        self.get(&format!("{base}/firewall/options")).await
    }

    pub async fn set_firewall_options(
        &self,
        base: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.put(&format!("{base}/firewall/options"), params).await
    }

    pub async fn vncproxy(&self, node: &str, kind: GuestKind, vmid: u32) -> Result<VncProxy> {
        let mut params = HashMap::new();
        // websocket=1 makes the proxy speak websocket for embedding.
        params.insert("websocket".to_string(), "1".to_string());
        self.post(
            &format!("/nodes/{node}/{}/{vmid}/vncproxy", kind.as_path()),
            &params,
        )
        .await
    }

    pub async fn termproxy(&self, node: &str, kind: GuestKind, vmid: u32) -> Result<TermProxy> {
        self.post(
            &format!("/nodes/{node}/{}/{vmid}/termproxy", kind.as_path()),
            &HashMap::new(),
        )
        .await
    }
}
