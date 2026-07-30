use serde::{Deserialize, Serialize};

/// Wrapper every Proxmox API response uses: `{"data": ...}`.
///
/// A few endpoints hang extra attributes off the envelope beside `data`.
/// `changes` is the only one this app reads — see `Client::node_network`.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
    #[serde(default)]
    pub changes: Option<String>,
}

/// One entry from `GET /cluster/resources`. Fields vary by `type`
/// ("node" | "qemu" | "lxc" | "storage"); absent ones are None.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResource {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub node: Option<String>,
    pub vmid: Option<u32>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub template: Option<u8>,
    pub cpu: Option<f64>,
    pub maxcpu: Option<f64>,
    pub mem: Option<u64>,
    pub maxmem: Option<u64>,
    pub disk: Option<u64>,
    pub maxdisk: Option<u64>,
    pub uptime: Option<u64>,
    pub storage: Option<String>,
    pub netin: Option<u64>,
    pub netout: Option<u64>,
}

/// One entry from `GET /nodes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    pub node: String,
    pub status: String,
    pub cpu: Option<f64>,
    pub maxcpu: Option<f64>,
    pub mem: Option<u64>,
    pub maxmem: Option<u64>,
    pub disk: Option<u64>,
    pub maxdisk: Option<u64>,
    pub uptime: Option<u64>,
}

/// `GET /version` — used as the test-connection probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub release: String,
}

/// Guest kind — Proxmox calls them qemu (VM) and lxc (container).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuestKind {
    Qemu,
    Lxc,
}

impl GuestKind {
    pub fn as_path(&self) -> &'static str {
        match self {
            GuestKind::Qemu => "qemu",
            GuestKind::Lxc => "lxc",
        }
    }
}

/// Power actions on a guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Start,
    Stop,
    Reboot,
    Shutdown,
}

impl PowerAction {
    pub fn as_path(&self) -> &'static str {
        match self {
            PowerAction::Start => "start",
            PowerAction::Stop => "stop",
            PowerAction::Reboot => "reboot",
            PowerAction::Shutdown => "shutdown",
        }
    }
}

/// One entry from `GET /nodes/{node}/tasks`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub upid: String,
    pub node: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: Option<String>,
    pub starttime: Option<u64>,
    pub endtime: Option<u64>,
    pub user: Option<String>,
    pub id: Option<String>,
}

/// One line from `GET /nodes/{node}/tasks/{upid}/log`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogLine {
    pub n: u64,
    pub t: String,
}

/// `GET /nodes/{node}/tasks/{upid}/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub upid: String,
    pub status: String,
    pub exitstatus: Option<String>,
}

/// One entry from `GET /nodes/{node}/network`.
///
/// Every field past `kind` is optional and type-dependent. They are all
/// declared because `PUT /nodes/{node}/network/{iface}` *replaces* the
/// interface definition with whatever the body carries — a field the edit
/// form cannot see is a field the next save silently drops.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub iface: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub method: Option<String>,
    pub address: Option<String>,
    pub netmask: Option<String>,
    pub cidr: Option<String>,
    pub gateway: Option<String>,
    pub cidr6: Option<String>,
    pub gateway6: Option<String>,
    pub bridge_ports: Option<String>,
    pub bridge_vlan_aware: Option<u8>,
    pub slaves: Option<String>,
    pub bond_mode: Option<String>,
    pub bond_xmit_hash_policy: Option<String>,
    #[serde(rename = "vlan-id")]
    pub vlan_id: Option<u32>,
    #[serde(rename = "vlan-raw-device")]
    pub vlan_raw_device: Option<String>,
    pub mtu: Option<u32>,
    pub comments: Option<String>,
    pub active: Option<u8>,
    pub autostart: Option<u8>,
}

/// `GET /nodes/{node}/network` in full: the interface list plus the diff of
/// any edits staged in `/etc/network/interfaces.new`. `changes` is None when
/// nothing is pending — that is the signal the Apply/Revert controls key off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeNetwork {
    pub interfaces: Vec<NetworkInterface>,
    pub changes: Option<String>,
}

/// One entry from `GET /nodes/{node}/storage/{storage}/content`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageContent {
    pub volid: String,
    pub content: String,
    pub format: Option<String>,
    pub size: Option<u64>,
    pub vmid: Option<u32>,
    pub ctime: Option<u64>,
    pub notes: Option<String>,
}

/// One scheduled job from `GET /cluster/backup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJob {
    pub id: String,
    pub schedule: Option<String>,
    pub storage: Option<String>,
    /// Comma-separated vmid list; absent when `all` is set.
    pub vmid: Option<String>,
    pub all: Option<u8>,
    pub enabled: Option<u8>,
    pub mode: Option<String>,
    pub node: Option<String>,
}

/// One rule from `GET {cluster|node|guest}/firewall/rules`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub pos: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub action: String,
    pub enable: Option<u8>,
    pub proto: Option<String>,
    pub dport: Option<String>,
    pub sport: Option<String>,
    pub source: Option<String>,
    pub dest: Option<String>,
    pub iface: Option<String>,
    pub comment: Option<String>,
}

/// One job from `GET /cluster/replication`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationJob {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub guest: Option<u32>,
    pub target: Option<String>,
    pub schedule: Option<String>,
    pub disable: Option<u8>,
}

/// One entry from `GET /storage` — cluster-wide storage.cfg definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub storage: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub content: Option<String>,
    pub path: Option<String>,
    pub server: Option<String>,
    pub export: Option<String>,
    pub share: Option<String>,
    pub nodes: Option<String>,
    pub shared: Option<u8>,
    pub disable: Option<u8>,
}

/// One entry from `GET /nodes/{node}/storage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSummary {
    pub storage: String,
    pub content: Option<String>,
    pub active: Option<u8>,
    pub avail: Option<u64>,
    pub total: Option<u64>,
}

/// One entry from `GET /access/users`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessUser {
    pub userid: String,
    pub comment: Option<String>,
    pub enable: Option<u8>,
    pub expire: Option<u64>,
    pub email: Option<String>,
    pub groups: Option<serde_json::Value>,
}

/// One entry from `GET /access/domains` (auth realms).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessDomain {
    pub realm: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub comment: Option<String>,
    pub default: Option<u8>,
}

/// One entry from `GET /access/acl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ugid: String,
    pub roleid: String,
    pub propagate: Option<u8>,
}

/// One entry from `GET /access/roles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRole {
    pub roleid: String,
    pub privs: Option<String>,
    pub special: Option<u8>,
}

/// One entry from `GET /cluster/ha/resources`. `sid` is the HA service id,
/// `"qemu:100"` / `"lxc:101"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaResource {
    pub sid: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub state: Option<String>,
    pub group: Option<String>,
    pub comment: Option<String>,
    pub max_restart: Option<u32>,
    pub max_relocate: Option<u32>,
}

/// One entry from `GET /cluster/ha/groups`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaGroup {
    pub group: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Comma-separated `node[:priority]` list, e.g. "pve1:2,pve2".
    pub nodes: Option<String>,
    pub restricted: Option<u8>,
    pub nofailback: Option<u8>,
    pub comment: Option<String>,
}

/// One entry from `GET /cluster/ha/status/current`. Deliberately all-optional
/// past `id`: the list is heterogeneous, `kind` being "quorum" | "master" |
/// "lrm" | "service" decides which other fields the entry carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaStatus {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub status: Option<String>,
    pub node: Option<String>,
    /// String on some PVE versions, int on others — pass it through as-is.
    pub quorate: Option<serde_json::Value>,
    pub crm_state: Option<String>,
    pub timestamp: Option<u64>,
}

/// One entry from `GET /nodes/{node}/ceph/pool`. `pool` is the numeric id,
/// `pool_name` the name every other endpoint addresses the pool by.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CephPool {
    pub pool: Option<u32>,
    pub pool_name: String,
    pub size: Option<u32>,
    pub min_size: Option<u32>,
    pub pg_num: Option<u32>,
    /// The rule's numeric id on older PVE, its name on newer — as-is.
    pub crush_rule: Option<serde_json::Value>,
    pub crush_rule_name: Option<String>,
    pub percent_used: Option<f64>,
    pub bytes_used: Option<u64>,
    pub pg_autoscale_mode: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Which Ceph daemon listing to fetch: `/nodes/{node}/ceph/{mon|mgr|mds}`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CephServiceKind {
    Mon,
    Mgr,
    Mds,
}

impl CephServiceKind {
    pub fn as_path(&self) -> &'static str {
        match self {
            CephServiceKind::Mon => "mon",
            CephServiceKind::Mgr => "mgr",
            CephServiceKind::Mds => "mds",
        }
    }
}

/// Start/stop of a Ceph daemon. Not `PowerAction` — Ceph has no reboot or
/// shutdown, and those paths would 501.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CephDaemonAction {
    Start,
    Stop,
}

impl CephDaemonAction {
    pub fn as_path(&self) -> &'static str {
        match self {
            CephDaemonAction::Start => "start",
            CephDaemonAction::Stop => "stop",
        }
    }
}

/// One entry from `GET /nodes/{node}/certificates/info`, and the body
/// `POST /nodes/{node}/certificates/custom` answers with. Every field is
/// optional in the PVE schema, so nothing here is required — a listing must
/// degrade a row, never fail to decode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    /// `pve-ssl.pem` for the node's self-signed cert, `pveproxy-ssl.pem` for a
    /// custom or ACME one.
    pub filename: Option<String>,
    pub fingerprint: Option<String>,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    /// Unix epoch seconds.
    pub notbefore: Option<i64>,
    pub notafter: Option<i64>,
    pub san: Option<Vec<String>>,
    /// The certificate in PEM. The private key is never returned by the API.
    pub pem: Option<String>,
    /// PVE spells these two with hyphens; the alias covers the underscore
    /// spelling, and renaming on deserialize only keeps the field snake_case
    /// on the way out to the frontend.
    #[serde(rename(deserialize = "public-key-type"), alias = "public_key_type")]
    pub public_key_type: Option<String>,
    #[serde(rename(deserialize = "public-key-bits"), alias = "public_key_bits")]
    pub public_key_bits: Option<u32>,
}

/// One entry from `GET /cluster/acme/account`. The listing carries the name
/// only — directory, contacts and ToS need the per-account detail call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeAccountEntry {
    pub name: String,
}

/// `POST /nodes/{node}/{qemu|lxc}/{vmid}/vncproxy` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VncProxy {
    pub ticket: String,
    pub port: serde_json::Value,
    pub user: Option<String>,
    pub cert: Option<String>,
}

/// `POST /nodes/{node}/{qemu|lxc}/{vmid}/termproxy` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermProxy {
    pub ticket: String,
    pub port: serde_json::Value,
    pub user: Option<String>,
}

/// `GET /access/permissions` — the calling token's own effective privileges,
/// keyed by ACL path then by privilege name. PVE sends `1` for every granted
/// privilege and omits the rest, so presence is the whole answer; the value is
/// carried through untouched rather than being flattened to a set, since the
/// UI passes the map straight to the frontend.
pub type Permissions = std::collections::HashMap<String, std::collections::HashMap<String, u8>>;
