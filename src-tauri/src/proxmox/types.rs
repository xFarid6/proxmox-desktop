use serde::{Deserialize, Serialize};

/// Wrapper every Proxmox API response uses: `{"data": ...}`.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
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
    pub bridge_ports: Option<String>,
    pub active: Option<u8>,
    pub autostart: Option<u8>,
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
