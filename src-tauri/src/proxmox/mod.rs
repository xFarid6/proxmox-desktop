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
        // Timeouts so a dead route (mobile network switch, tailnet peer
        // offline) fails fast instead of hanging the UI forever.
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(accept_invalid_certs)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
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

    async fn decode_envelope<T: DeserializeOwned>(
        resp: reqwest::Response,
    ) -> Result<ApiResponse<T>> {
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        serde_json::from_str(&body).map_err(|e| Error::Decode(e.to_string()))
    }

    async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        Ok(Self::decode_envelope::<T>(resp).await?.data)
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

    /// Same as `get` but keeps the envelope's sibling attributes.
    async fn get_envelope<T: DeserializeOwned>(&self, path: &str) -> Result<ApiResponse<T>> {
        let resp = self
            .http
            .get(self.url(path))
            .header("Authorization", &self.auth_header)
            .send()
            .await?;
        Self::decode_envelope(resp).await
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

    /// Interfaces on a node, plus the diff of any staged-but-unapplied edits.
    ///
    /// PVE never edits `/etc/network/interfaces` in place: writes land in
    /// `interfaces.new` and the index reports the diff against the live file
    /// as a `changes` attribute *beside* `data`. The whole apply/revert model
    /// hangs off that field, which is why this endpoint reads the envelope.
    pub async fn node_network(&self, node: &str) -> Result<NodeNetwork> {
        let env = self
            .get_envelope::<Vec<NetworkInterface>>(&format!("/nodes/{node}/network"))
            .await?;
        Ok(NodeNetwork {
            interfaces: env.data,
            changes: env.changes,
        })
    }

    /// Stage a new interface. Params are Proxmox's own: `iface`, `type`, then
    /// whatever that type takes — `bridge_ports`, `slaves`/`bond_mode`,
    /// `vlan-id`/`vlan-raw-device`, `cidr`, `gateway`, `mtu`, `autostart`.
    /// Nothing reaches the running system until `apply_network`.
    pub async fn create_network_iface(
        &self,
        node: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.post(&format!("/nodes/{node}/network"), params).await
    }

    /// Replace an interface's definition. `type` is required here as well as
    /// on create — PVE validates the body against the declared type rather
    /// than inferring it. Keys absent from `params` are dropped from the
    /// config, so callers must send the full definition, not a delta.
    pub async fn update_network_iface(
        &self,
        node: &str,
        iface: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.put(&format!("/nodes/{node}/network/{iface}"), params)
            .await
    }

    /// Stage removal of an interface.
    pub async fn delete_network_iface(&self, node: &str, iface: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/nodes/{node}/network/{iface}"))
            .await
    }

    /// Apply the staged config — `ifreload -a` on the node, as a task, so the
    /// UPID comes back. This is the call that can cut the management link if
    /// the staged config is wrong; the caller warns first.
    pub async fn apply_network(&self, node: &str) -> Result<String> {
        self.put(&format!("/nodes/{node}/network"), &HashMap::new())
            .await
    }

    /// Discard the staged config. The running config is untouched either way.
    pub async fn revert_network(&self, node: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/nodes/{node}/network")).await
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

    /// Refresh the node's APT package index, as a task — the UPID comes back.
    ///
    /// This is the one node-scoped job PVE will start on demand that changes
    /// nothing but a package index: no upgrade is installed, no service is
    /// restarted. It is what the Tasks tab offers as "start a task", since
    /// every other task-producing call in this client either belongs to
    /// another tab or touches guests. Needs Sys.Modify on the node.
    pub async fn apt_update(&self, node: &str) -> Result<String> {
        self.post(&format!("/nodes/{node}/apt/update"), &HashMap::new())
            .await
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

    pub async fn access_users(&self) -> Result<Vec<AccessUser>> {
        self.get("/access/users").await
    }

    /// Create a user (params: userid, password?, comment?, enable?).
    pub async fn add_user(&self, params: &HashMap<String, String>) -> Result<serde_json::Value> {
        self.post("/access/users", params).await
    }

    pub async fn delete_user(&self, userid: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/access/users/{userid}")).await
    }

    pub async fn access_domains(&self) -> Result<Vec<AccessDomain>> {
        self.get("/access/domains").await
    }

    pub async fn access_roles(&self) -> Result<Vec<AccessRole>> {
        self.get("/access/roles").await
    }

    pub async fn access_acl(&self) -> Result<Vec<AclEntry>> {
        self.get("/access/acl").await
    }

    /// What *this* token may do, per path — `{"/vms/100": {"VM.Backup": 1}, ...}`.
    ///
    /// Unlike `/access/acl` this needs no privilege of its own (any token may
    /// ask about itself), and it already accounts for role expansion and
    /// privilege separation, so it is the only honest way to answer "will this
    /// action be refused?" before attempting it. Paths are only listed where
    /// something is granted, and not every descendant is enumerated — a caller
    /// asking about `/vms/100` must also look at `/vms` and `/`. See
    /// `hasPrivilege` in `src/backup.ts`.
    pub async fn access_permissions(&self) -> Result<Permissions> {
        self.get("/access/permissions").await
    }

    /// Grant or revoke ACLs (params: path, roles, users|groups|tokens,
    /// delete=1 to revoke).
    pub async fn set_acl(&self, params: &HashMap<String, String>) -> Result<serde_json::Value> {
        self.put("/access/acl", params).await
    }

    /// Cluster-wide storage definitions (storage.cfg).
    pub async fn storage_configs(&self) -> Result<Vec<StorageConfig>> {
        self.get("/storage").await
    }

    /// Add a storage definition (params: storage, type, content, path/server/...).
    pub async fn add_storage(&self, params: &HashMap<String, String>) -> Result<serde_json::Value> {
        self.post("/storage", params).await
    }

    /// Remove a storage definition. Does not touch the data on it.
    pub async fn delete_storage(&self, storage: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/storage/{storage}")).await
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

    /// Guests managed by HA, cluster-wide.
    pub async fn ha_resources(&self) -> Result<Vec<HaResource>> {
        self.get("/cluster/ha/resources").await
    }

    /// Put a guest under HA (params: sid, state?, group?, max_restart?,
    /// max_relocate?). Creating posts to the collection with `sid` in the
    /// body; updating puts to the sid's own path — see `update_ha_resource`.
    pub async fn add_ha_resource(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.post("/cluster/ha/resources", params).await
    }

    pub async fn update_ha_resource(
        &self,
        sid: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.put(&format!("/cluster/ha/resources/{sid}"), params)
            .await
    }

    /// Take a guest out of HA. Does not touch the guest itself.
    pub async fn delete_ha_resource(&self, sid: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/cluster/ha/resources/{sid}"))
            .await
    }

    pub async fn ha_groups(&self) -> Result<Vec<HaGroup>> {
        self.get("/cluster/ha/groups").await
    }

    /// Create a group (params: group, nodes, restricted?, nofailback?).
    pub async fn add_ha_group(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.post("/cluster/ha/groups", params).await
    }

    pub async fn update_ha_group(
        &self,
        group: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        self.put(&format!("/cluster/ha/groups/{group}"), params)
            .await
    }

    pub async fn delete_ha_group(&self, group: &str) -> Result<serde_json::Value> {
        self.delete_req(&format!("/cluster/ha/groups/{group}"))
            .await
    }

    /// Live HA state: quorum, the CRM master, each node's LRM, each service.
    pub async fn ha_status_current(&self) -> Result<Vec<HaStatus>> {
        self.get("/cluster/ha/status/current").await
    }

    /// Ceph health, mon quorum, PG states and capacity. The blob is deep and
    /// version-dependent, so it stays a raw Value and the UI reads the handful
    /// of fields it needs. Every Ceph endpoint is per-node even though the
    /// data is cluster-wide — the node just has to be a Ceph member.
    ///
    /// On a node without Ceph this errors (500 "rados_connect failed", 501 on
    /// older PVE). That failure is how the app detects Ceph at all.
    pub async fn ceph_status(&self, node: &str) -> Result<serde_json::Value> {
        self.get(&format!("/nodes/{node}/ceph/status")).await
    }

    /// The CRUSH tree, `{root: {children: [...]}}`. Bucket nesting is not
    /// fixed — rack and datacenter buckets are legal between root and host —
    /// so it is walked in the UI rather than modelled here.
    pub async fn ceph_osds(&self, node: &str) -> Result<serde_json::Value> {
        self.get(&format!("/nodes/{node}/ceph/osd")).await
    }

    pub async fn ceph_pools(&self, node: &str) -> Result<Vec<CephPool>> {
        self.get(&format!("/nodes/{node}/ceph/pool")).await
    }

    /// MON, MGR or MDS listing. One method: the three paths differ only in
    /// their per-daemon fields, which the UI renders generically.
    pub async fn ceph_services(
        &self,
        node: &str,
        kind: CephServiceKind,
    ) -> Result<serde_json::Value> {
        self.get(&format!("/nodes/{node}/ceph/{}", kind.as_path()))
            .await
    }

    /// Mark an OSD back into the cluster. Data rebalances onto it afterwards;
    /// the call itself returns immediately with no task.
    pub async fn ceph_osd_in(&self, node: &str, osdid: u32) -> Result<Option<String>> {
        self.post(
            &format!("/nodes/{node}/ceph/osd/{osdid}/in"),
            &HashMap::new(),
        )
        .await
    }

    /// Mark an OSD out. The daemon keeps running; its data drains elsewhere.
    pub async fn ceph_osd_out(&self, node: &str, osdid: u32) -> Result<Option<String>> {
        self.post(
            &format!("/nodes/{node}/ceph/osd/{osdid}/out"),
            &HashMap::new(),
        )
        .await
    }

    /// Start or stop the OSD's daemon. Quirk: there is no per-OSD start/stop
    /// path. Proxmox drives every Ceph daemon through the node-wide
    /// `POST /nodes/{node}/ceph/{start|stop}` with `service=osd.{id}` —
    /// omitting `service` there would hit the whole `ceph.target` instead.
    pub async fn ceph_osd_power(
        &self,
        node: &str,
        osdid: u32,
        action: CephDaemonAction,
    ) -> Result<Option<String>> {
        let mut params = HashMap::new();
        params.insert("service".to_string(), format!("osd.{osdid}"));
        self.post(&format!("/nodes/{node}/ceph/{}", action.as_path()), &params)
            .await
    }

    /// Destroy an OSD. `cleanup` also wipes the disk's partition table so the
    /// device can be reused. Rides the query string because `delete_req`
    /// sends no body — same as `storage_content`.
    pub async fn ceph_osd_destroy(
        &self,
        node: &str,
        osdid: u32,
        cleanup: bool,
    ) -> Result<Option<String>> {
        let mut path = format!("/nodes/{node}/ceph/osd/{osdid}");
        if cleanup {
            path.push_str("?cleanup=1");
        }
        self.delete_req(&path).await
    }

    /// Create a pool (params: name, size, min_size, pg_num, crush_rule,
    /// pg_autoscale_mode, add_storages).
    pub async fn ceph_pool_create(
        &self,
        node: &str,
        params: &HashMap<String, String>,
    ) -> Result<Option<String>> {
        self.post(&format!("/nodes/{node}/ceph/pool"), params).await
    }

    pub async fn ceph_pool_update(
        &self,
        node: &str,
        name: &str,
        params: &HashMap<String, String>,
    ) -> Result<Option<String>> {
        self.put(&format!("/nodes/{node}/ceph/pool/{name}"), params)
            .await
    }

    /// Delete a pool and everything in it. `remove_storages` also drops the
    /// PVE storage entries backed by it.
    pub async fn ceph_pool_delete(
        &self,
        node: &str,
        name: &str,
        remove_storages: bool,
    ) -> Result<Option<String>> {
        let mut path = format!("/nodes/{node}/ceph/pool/{name}");
        if remove_storages {
            path.push_str("?remove_storages=1");
        }
        self.delete_req(&path).await
    }

    /// The certificates pveproxy serves for this node: always the self-signed
    /// `pve-ssl.pem`, plus `pveproxy-ssl.pem` once a custom or ACME cert has
    /// replaced it.
    pub async fn certificates_info(&self, node: &str) -> Result<Vec<CertificateInfo>> {
        self.get(&format!("/nodes/{node}/certificates/info")).await
    }

    /// Install a custom certificate. Params: `certificates` (PEM chain), `key`
    /// (PEM private key), `force=1` to overwrite an existing custom cert,
    /// `restart=1` to restart pveproxy so it is served immediately. `params`
    /// carries private key material — it must never be logged.
    pub async fn upload_certificate(
        &self,
        node: &str,
        params: &HashMap<String, String>,
    ) -> Result<CertificateInfo> {
        self.post(&format!("/nodes/{node}/certificates/custom"), params)
            .await
    }

    /// Drop the custom certificate, reverting the node to its self-signed one.
    /// `restart` rides the query string because `delete_req` sends no body —
    /// same as `ceph_pool_delete`.
    pub async fn delete_custom_certificate(
        &self,
        node: &str,
        restart: bool,
    ) -> Result<Option<String>> {
        let mut path = format!("/nodes/{node}/certificates/custom");
        if restart {
            path.push_str("?restart=1");
        }
        self.delete_req(&path).await
    }

    /// Order the certificate for the ACME config on this node. Returns a task
    /// UPID — the new cert is only in place once that task finishes, and
    /// pveproxy is restarted by the task itself.
    pub async fn acme_order_certificate(&self, node: &str) -> Result<String> {
        self.post(
            &format!("/nodes/{node}/certificates/acme/certificate"),
            &HashMap::new(),
        )
        .await
    }

    /// Renew the ACME certificate. PVE refuses while it is more than 30 days
    /// from expiry unless `force`. Returns a task UPID.
    pub async fn acme_renew_certificate(&self, node: &str, force: bool) -> Result<String> {
        let mut params = HashMap::new();
        if force {
            params.insert("force".to_string(), "1".to_string());
        }
        self.put(
            &format!("/nodes/{node}/certificates/acme/certificate"),
            &params,
        )
        .await
    }

    /// ACME accounts are cluster-wide, not per-node. Names only.
    pub async fn acme_accounts(&self) -> Result<Vec<AcmeAccountEntry>> {
        self.get("/cluster/acme/account").await
    }

    /// One account's registration: directory, contacts, ToS and the upstream
    /// account object. The last of those is whatever the ACME server returns,
    /// so the whole thing stays raw.
    pub async fn acme_account(&self, name: &str) -> Result<serde_json::Value> {
        self.get(&format!("/cluster/acme/account/{name}")).await
    }

    /// Configured ACME challenge plugins. Read-only on purpose — plugin
    /// configuration is out of scope for #20. Per-plugin fields depend on the
    /// DNS API behind them, hence the raw Value.
    pub async fn acme_plugins(&self) -> Result<serde_json::Value> {
        self.get("/cluster/acme/plugins").await
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
