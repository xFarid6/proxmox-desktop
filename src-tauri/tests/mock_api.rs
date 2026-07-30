//! Integration tests against a mocked Proxmox API.
//! No live cluster exists in CI — these verify request shape, auth header,
//! response decoding, and error mapping against recorded fixture bodies.

use std::collections::HashMap;

use proxmox_desktop_lib::proxmox::types::{
    CephDaemonAction, CephServiceKind, GuestKind, PowerAction,
};
use proxmox_desktop_lib::proxmox::{Client, Error};
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "root@pam!desktop=aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

async fn client(server: &MockServer) -> Client {
    Client::new(&server.uri(), TOKEN, false).unwrap()
}

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json")
}

#[tokio::test]
async fn version_sends_token_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/version"))
        .and(header("Authorization", format!("PVEAPIToken={TOKEN}")))
        .respond_with(json(r#"{"data":{"version":"8.2.4","release":"8.2"}}"#))
        .expect(1)
        .mount(&server)
        .await;

    let v = client(&server).await.version().await.unwrap();
    assert_eq!(v.version, "8.2.4");
}

#[tokio::test]
async fn cluster_resources_decodes_mixed_types() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/resources"))
        .respond_with(json(
            r#"{"data":[
                {"id":"node/pve1","type":"node","node":"pve1","status":"online","cpu":0.02,"maxcpu":8,"mem":4294967296,"maxmem":16777216000,"uptime":123456},
                {"id":"qemu/100","type":"qemu","node":"pve1","vmid":100,"name":"web01","status":"running","cpu":0.1,"maxcpu":2,"mem":1073741824,"maxmem":2147483648},
                {"id":"lxc/101","type":"lxc","node":"pve1","vmid":101,"name":"db01","status":"stopped","template":0},
                {"id":"storage/pve1/local","type":"storage","node":"pve1","storage":"local","disk":1000,"maxdisk":10000}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let res = client(&server).await.cluster_resources().await.unwrap();
    assert_eq!(res.len(), 4);
    assert_eq!(res[0].kind, "node");
    assert_eq!(res[1].vmid, Some(100));
    assert_eq!(res[1].name.as_deref(), Some("web01"));
    assert_eq!(res[2].status.as_deref(), Some("stopped"));
    assert_eq!(res[3].storage.as_deref(), Some("local"));
}

#[tokio::test]
async fn power_action_posts_and_returns_upid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/qemu/100/status/start"))
        .respond_with(json(r#"{"data":"UPID:pve1:0001:qmstart:100:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let upid = client(&server)
        .await
        .power("pve1", GuestKind::Qemu, 100, PowerAction::Start)
        .await
        .unwrap();
    assert!(upid.starts_with("UPID:pve1"));
}

#[tokio::test]
async fn lxc_shutdown_uses_lxc_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/lxc/101/status/shutdown"))
        .respond_with(json(
            r#"{"data":"UPID:pve1:0002:vzshutdown:101:root@pam:"}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .await
        .power("pve1", GuestKind::Lxc, 101, PowerAction::Shutdown)
        .await
        .unwrap();
}

#[tokio::test]
async fn api_error_maps_status_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/version"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string(r#"{"message":"authentication failure"}"#),
        )
        .mount(&server)
        .await;

    let err = client(&server).await.version().await.unwrap_err();
    match err {
        Error::Api { status, message } => {
            assert_eq!(status, 401);
            assert!(message.contains("authentication failure"));
        }
        other => panic!("expected Api error, got {other:?}"),
    }
}

#[tokio::test]
async fn set_config_qemu_posts_form() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/qemu/100/config"))
        .and(body_string_contains("cores=4"))
        .and(body_string_contains("memory=8192"))
        .respond_with(json(r#"{"data":"UPID:pve1:0003:qmconfig:100:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let mut params = HashMap::new();
    params.insert("cores".to_string(), "4".to_string());
    params.insert("memory".to_string(), "8192".to_string());
    let upid = client(&server)
        .await
        .set_guest_config("pve1", GuestKind::Qemu, 100, &params)
        .await
        .unwrap();
    assert!(upid.unwrap().starts_with("UPID:"));
}

#[tokio::test]
async fn set_config_lxc_puts_and_returns_none() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/lxc/101/config"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let mut params = HashMap::new();
    params.insert("cores".to_string(), "2".to_string());
    let upid = client(&server)
        .await
        .set_guest_config("pve1", GuestKind::Lxc, 101, &params)
        .await
        .unwrap();
    assert!(upid.is_none());
}

#[tokio::test]
async fn resize_disk_sends_disk_and_size() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/qemu/100/resize"))
        .and(body_string_contains("disk=scsi0"))
        .and(body_string_contains("size=%2B5G"))
        .respond_with(json(r#"{"data":"UPID:pve1:0004:qmresize:100:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .await
        .resize_disk("pve1", GuestKind::Qemu, 100, "scsi0", "+5G")
        .await
        .unwrap();
}

#[tokio::test]
async fn create_guest_posts_params() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/qemu"))
        .and(body_string_contains("vmid=105"))
        .respond_with(json(r#"{"data":"UPID:pve1:0005:qmcreate:105:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let mut params = HashMap::new();
    params.insert("vmid".to_string(), "105".to_string());
    params.insert("cores".to_string(), "2".to_string());
    let upid = client(&server)
        .await
        .create_guest("pve1", GuestKind::Qemu, &params)
        .await
        .unwrap();
    assert!(upid.contains("qmcreate"));
}

#[tokio::test]
async fn storage_content_filters_by_content_type() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/storage/local/content"))
        .and(query_param("content", "iso"))
        .respond_with(json(
            r#"{"data":[{"volid":"local:iso/debian-12.iso","content":"iso","format":"iso","size":650000000}]}"#,
        ))
        .mount(&server)
        .await;

    let items = client(&server)
        .await
        .storage_content("pve1", "local", Some("iso"))
        .await
        .unwrap();
    assert_eq!(items[0].volid, "local:iso/debian-12.iso");
}

#[tokio::test]
async fn task_log_and_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/tasks/UPID:pve1:0001/status"))
        .respond_with(json(
            r#"{"data":{"upid":"UPID:pve1:0001","status":"stopped","exitstatus":"OK"}}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/tasks/UPID:pve1:0001/log"))
        .and(query_param("start", "0"))
        .respond_with(json(
            r#"{"data":[{"n":1,"t":"starting task"},{"n":2,"t":"TASK OK"}]}"#,
        ))
        .mount(&server)
        .await;

    let c = client(&server).await;
    let st = c.task_status("pve1", "UPID:pve1:0001").await.unwrap();
    assert_eq!(st.exitstatus.as_deref(), Some("OK"));
    let log = c.task_log("pve1", "UPID:pve1:0001", 0).await.unwrap();
    assert_eq!(log.len(), 2);
}

#[tokio::test]
async fn vncproxy_requests_websocket() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/qemu/100/vncproxy"))
        .and(body_string_contains("websocket=1"))
        .respond_with(json(
            r#"{"data":{"ticket":"PVEVNC:ticket","port":"5900","user":"root@pam"}}"#,
        ))
        .mount(&server)
        .await;

    let p = client(&server)
        .await
        .vncproxy("pve1", GuestKind::Qemu, 100)
        .await
        .unwrap();
    assert_eq!(p.ticket, "PVEVNC:ticket");
}

#[tokio::test]
async fn network_interfaces_decode() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/network"))
        .respond_with(json(
            r#"{"data":[
                {"iface":"vmbr0","type":"bridge","method":"static","address":"192.168.1.10","cidr":"192.168.1.10/24","gateway":"192.168.1.1","bridge_ports":"eno1","active":1,"autostart":1},
                {"iface":"eno1","type":"eth","method":"manual","active":1}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let net = client(&server).await.node_network("pve1").await.unwrap();
    assert_eq!(net.interfaces.len(), 2);
    assert_eq!(net.interfaces[0].kind, "bridge");
    assert_eq!(net.interfaces[0].bridge_ports.as_deref(), Some("eno1"));
    assert!(net.changes.is_none());
}

/// The `changes` field sits beside `data` in the envelope, not on each
/// interface — this is the diff PVE reports for staged-but-unapplied edits,
/// and it's what the apply/revert UI keys off of.
#[tokio::test]
async fn network_edit_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/network"))
        .respond_with(json(
            r#"{"data":[{"iface":"vmbr1","type":"bridge","bridge_ports":"eno2","autostart":1,"mtu":9000}],"changes":"--- /etc/network/interfaces\n+++ /etc/network/interfaces.new\n+auto vmbr1\n"}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/network"))
        .and(body_string_contains("type=bridge"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/network/vmbr1"))
        .and(body_string_contains("mtu=9000"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/nodes/pve1/network/vmbr1"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/network"))
        .respond_with(json(r#"{"data":"UPID:pve1:0000:network:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/nodes/pve1/network"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let net = c.node_network("pve1").await.unwrap();
    let changes = net.changes.unwrap();
    assert!(changes.contains("vmbr1"));
    assert_eq!(net.interfaces[0].mtu, Some(9000));
    assert_eq!(net.interfaces[0].bridge_ports.as_deref(), Some("eno2"));

    let mut params = HashMap::new();
    params.insert("iface".to_string(), "vmbr1".to_string());
    params.insert("type".to_string(), "bridge".to_string());
    c.create_network_iface("pve1", &params).await.unwrap();

    let mut params = HashMap::new();
    params.insert("mtu".to_string(), "9000".to_string());
    c.update_network_iface("pve1", "vmbr1", &params)
        .await
        .unwrap();

    c.delete_network_iface("pve1", "vmbr1").await.unwrap();

    let upid = c.apply_network("pve1").await.unwrap();
    assert!(upid.starts_with("UPID:"));

    c.revert_network("pve1").await.unwrap();
}

#[tokio::test]
async fn vzdump_posts_params_and_returns_upid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/vzdump"))
        .and(body_string_contains("vmid=100"))
        .and(body_string_contains("storage=local"))
        .respond_with(json(r#"{"data":"UPID:pve1:0002:vzdump:100:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let mut params = HashMap::new();
    params.insert("vmid".to_string(), "100".to_string());
    params.insert("storage".to_string(), "local".to_string());
    let upid = client(&server).await.vzdump("pve1", &params).await.unwrap();
    assert!(upid.contains("vzdump"));
}

#[tokio::test]
async fn delete_volume_uses_delete_method() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/api2/json/nodes/pve1/storage/local/content/local:backup/vzdump-qemu-100.vma.zst",
        ))
        .respond_with(json(r#"{"data":"UPID:pve1:0003:imgdel:root@pam:"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let upid = client(&server)
        .await
        .delete_volume("pve1", "local", "local:backup/vzdump-qemu-100.vma.zst")
        .await
        .unwrap();
    assert!(upid.unwrap().contains("imgdel"));
}

#[tokio::test]
async fn backup_and_replication_jobs_decode() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/backup"))
        .respond_with(json(
            r#"{"data":[{"id":"backup-1","schedule":"sun 03:00","storage":"local","vmid":"100,101","enabled":1,"mode":"snapshot"}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/replication"))
        .respond_with(json(
            r#"{"data":[{"id":"100-0","type":"local","guest":100,"target":"pve2","schedule":"*/15"}]}"#,
        ))
        .mount(&server)
        .await;

    let c = client(&server).await;
    let jobs = c.backup_jobs().await.unwrap();
    assert_eq!(jobs[0].vmid.as_deref(), Some("100,101"));
    let reps = c.replication_jobs().await.unwrap();
    assert_eq!(reps[0].target.as_deref(), Some("pve2"));
}

#[tokio::test]
async fn firewall_rules_scope_paths_and_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/firewall/rules"))
        .respond_with(json(
            r#"{"data":[{"pos":0,"type":"in","action":"ACCEPT","enable":1,"proto":"tcp","dport":"22","comment":"ssh"}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/qemu/100/firewall/rules"))
        .and(body_string_contains("action=DROP"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/cluster/firewall/rules/0"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let rules = c.firewall_rules("/cluster").await.unwrap();
    assert_eq!(rules[0].dport.as_deref(), Some("22"));

    let mut params = HashMap::new();
    params.insert("type".to_string(), "in".to_string());
    params.insert("action".to_string(), "DROP".to_string());
    c.add_firewall_rule("/nodes/pve1/qemu/100", &params)
        .await
        .unwrap();
    c.delete_firewall_rule("/cluster", 0).await.unwrap();
}

#[tokio::test]
async fn storage_configs_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/storage"))
        .respond_with(json(
            r#"{"data":[{"storage":"local","type":"dir","path":"/var/lib/vz","content":"iso,backup"},{"storage":"nas","type":"nfs","server":"10.0.0.5","export":"/srv/nfs","shared":1}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/storage"))
        .and(body_string_contains("type=nfs"))
        .respond_with(json(r#"{"data":{"storage":"nas2","type":"nfs"}}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/storage/nas"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let cfgs = c.storage_configs().await.unwrap();
    assert_eq!(cfgs[1].server.as_deref(), Some("10.0.0.5"));

    let mut params = HashMap::new();
    params.insert("storage".to_string(), "nas2".to_string());
    params.insert("type".to_string(), "nfs".to_string());
    c.add_storage(&params).await.unwrap();
    c.delete_storage("nas").await.unwrap();
}

#[tokio::test]
async fn ha_resources_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/ha/resources"))
        .respond_with(json(
            r#"{"data":[{"sid":"qemu:100","type":"vm","state":"started","group":"prod","max_restart":1,"max_relocate":2}]}"#,
        ))
        .mount(&server)
        .await;
    // Create posts the sid in the body; update puts it in the path.
    Mock::given(method("POST"))
        .and(path("/api2/json/cluster/ha/resources"))
        .and(body_string_contains("sid=lxc%3A101"))
        .and(body_string_contains("state=started"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/cluster/ha/resources/qemu:100"))
        .and(body_string_contains("state=stopped"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/cluster/ha/resources/qemu:100"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let res = c.ha_resources().await.unwrap();
    assert_eq!(res[0].sid, "qemu:100");
    assert_eq!(res[0].group.as_deref(), Some("prod"));
    assert_eq!(res[0].max_relocate, Some(2));

    let mut params = HashMap::new();
    params.insert("sid".to_string(), "lxc:101".to_string());
    params.insert("state".to_string(), "started".to_string());
    c.add_ha_resource(&params).await.unwrap();

    let mut params = HashMap::new();
    params.insert("state".to_string(), "stopped".to_string());
    c.update_ha_resource("qemu:100", &params).await.unwrap();
    c.delete_ha_resource("qemu:100").await.unwrap();
}

#[tokio::test]
async fn ha_groups_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/ha/groups"))
        .respond_with(json(
            r#"{"data":[{"group":"prod","type":"group","nodes":"pve1:2,pve2:1,pve3","restricted":1}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/cluster/ha/groups"))
        .and(body_string_contains("group=edge"))
        .and(body_string_contains("nofailback=1"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/cluster/ha/groups/prod"))
        .and(body_string_contains("restricted=0"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/cluster/ha/groups/prod"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let groups = c.ha_groups().await.unwrap();
    assert_eq!(groups[0].nodes.as_deref(), Some("pve1:2,pve2:1,pve3"));
    assert_eq!(groups[0].restricted, Some(1));

    let mut params = HashMap::new();
    params.insert("group".to_string(), "edge".to_string());
    params.insert("nodes".to_string(), "pve1".to_string());
    params.insert("nofailback".to_string(), "1".to_string());
    c.add_ha_group(&params).await.unwrap();

    let mut params = HashMap::new();
    params.insert("restricted".to_string(), "0".to_string());
    c.update_ha_group("prod", &params).await.unwrap();
    c.delete_ha_group("prod").await.unwrap();
}

#[tokio::test]
async fn ha_status_current_decodes_heterogeneous_entries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/ha/status/current"))
        .respond_with(json(
            r#"{"data":[
                {"id":"quorum","type":"quorum","status":"OK","quorate":"1"},
                {"id":"master:pve1","type":"master","status":"active","node":"pve1","timestamp":1700000000},
                {"id":"lrm:pve2","type":"lrm","status":"active","node":"pve2"},
                {"id":"service:qemu:100","type":"service","status":"started","node":"pve1","crm_state":"started"}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let st = client(&server).await.ha_status_current().await.unwrap();
    assert_eq!(st.len(), 4);
    assert_eq!(st[0].quorate.as_ref().unwrap(), "1");
    assert_eq!(st[1].node.as_deref(), Some("pve1"));
    assert_eq!(st[3].crm_state.as_deref(), Some("started"));
}

#[tokio::test]
async fn ceph_status_osds_and_services_decode() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/ceph/status"))
        .respond_with(json(
            r#"{"data":{"health":{"status":"HEALTH_WARN"},"quorum_names":["pve1","pve2"],
                "pgmap":{"num_pgs":129,"bytes_total":1000,"bytes_used":400,
                "pgs_by_state":[{"state_name":"active+clean","count":128}]}}}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/ceph/osd"))
        .respond_with(json(
            r#"{"data":{"root":{"name":"default","type":"root","children":[
                {"name":"pve1","type":"host","children":[
                    {"id":0,"name":"osd.0","type":"osd","status":"up","in":1,"device_class":"ssd"}]}]}}}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/ceph/mon"))
        .respond_with(json(
            r#"{"data":[{"name":"pve1","quorum":1,"host":"pve1"}]}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let st = c.ceph_status("pve1").await.unwrap();
    assert_eq!(st["health"]["status"], "HEALTH_WARN");
    assert_eq!(st["pgmap"]["pgs_by_state"][0]["count"], 128);

    let tree = c.ceph_osds("pve1").await.unwrap();
    assert_eq!(tree["root"]["children"][0]["children"][0]["name"], "osd.0");

    let mons = c.ceph_services("pve1", CephServiceKind::Mon).await.unwrap();
    assert_eq!(mons[0]["quorum"], 1);
}

/// A node without Ceph answers `/ceph/status` with an error, which is exactly
/// what the frontend's Ceph probe keys off.
#[tokio::test]
async fn ceph_status_errors_on_a_node_without_ceph() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/ceph/status"))
        .respond_with(ResponseTemplate::new(500).set_body_string("rados_connect failed"))
        .mount(&server)
        .await;

    let err = client(&server).await.ceph_status("pve1").await.unwrap_err();
    match err {
        Error::Api { status, .. } => assert_eq!(status, 500),
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
async fn ceph_osd_ops_hit_the_right_paths() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/ceph/osd/3/out"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/ceph/osd/3/in"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    // Start/stop go through the node-wide endpoint with service=osd.N.
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/ceph/stop"))
        .and(body_string_contains("service=osd.3"))
        .respond_with(json(r#"{"data":"UPID:pve1:stop"}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/nodes/pve1/ceph/osd/3"))
        .and(query_param("cleanup", "1"))
        .respond_with(json(r#"{"data":"UPID:pve1:destroyosd"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    c.ceph_osd_out("pve1", 3).await.unwrap();
    c.ceph_osd_in("pve1", 3).await.unwrap();
    assert_eq!(
        c.ceph_osd_power("pve1", 3, CephDaemonAction::Stop)
            .await
            .unwrap()
            .as_deref(),
        Some("UPID:pve1:stop")
    );
    assert_eq!(
        c.ceph_osd_destroy("pve1", 3, true)
            .await
            .unwrap()
            .as_deref(),
        Some("UPID:pve1:destroyosd")
    );
}

#[tokio::test]
async fn ceph_pools_crud() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/ceph/pool"))
        .respond_with(json(
            r#"{"data":[{"pool":2,"pool_name":"vmdata","size":3,"min_size":2,"pg_num":128,
                "crush_rule":0,"crush_rule_name":"replicated_rule","percent_used":0.12,
                "bytes_used":123456789,"type":"replicated"}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/ceph/pool"))
        .and(body_string_contains("name=fast"))
        .and(body_string_contains("min_size=2"))
        .respond_with(json(r#"{"data":"UPID:pve1:createpool"}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/ceph/pool/vmdata"))
        .and(body_string_contains("size=2"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/nodes/pve1/ceph/pool/vmdata"))
        .and(query_param("remove_storages", "1"))
        .respond_with(json(r#"{"data":"UPID:pve1:destroypool"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let pools = c.ceph_pools("pve1").await.unwrap();
    assert_eq!(pools[0].pool_name, "vmdata");
    assert_eq!(pools[0].min_size, Some(2));
    assert_eq!(pools[0].crush_rule_name.as_deref(), Some("replicated_rule"));
    assert_eq!(pools[0].percent_used, Some(0.12));

    let mut params = HashMap::new();
    params.insert("name".to_string(), "fast".to_string());
    params.insert("size".to_string(), "3".to_string());
    params.insert("min_size".to_string(), "2".to_string());
    c.ceph_pool_create("pve1", &params).await.unwrap();

    let mut params = HashMap::new();
    params.insert("size".to_string(), "2".to_string());
    c.ceph_pool_update("pve1", "vmdata", &params).await.unwrap();
    c.ceph_pool_delete("pve1", "vmdata", true).await.unwrap();
}

#[tokio::test]
async fn certificates_info_decodes_hyphenated_and_partial_entries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve1/certificates/info"))
        .respond_with(json(
            r#"{"data":[
                {"filename":"pve-ssl.pem","fingerprint":"AA:BB","subject":"CN=pve1.local",
                 "issuer":"CN=Proxmox Virtual Environment","notbefore":1700000000,
                 "notafter":1800000000,"san":["pve1","pve1.local"],
                 "public-key-type":"rsa","public-key-bits":2048,"pem":"-----BEGIN CERTIFICATE-----"},
                {"filename":"pveproxy-ssl.pem"}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let certs = client(&server)
        .await
        .certificates_info("pve1")
        .await
        .unwrap();
    assert_eq!(certs.len(), 2);
    assert_eq!(certs[0].public_key_type.as_deref(), Some("rsa"));
    assert_eq!(certs[0].public_key_bits, Some(2048));
    assert_eq!(certs[0].notafter, Some(1800000000));
    assert_eq!(
        certs[0].san.as_deref(),
        Some(&["pve1".to_string(), "pve1.local".to_string()][..])
    );
    // A node that reports nothing but a filename must still decode.
    assert_eq!(certs[1].filename.as_deref(), Some("pveproxy-ssl.pem"));
    assert!(certs[1].subject.is_none());
}

#[tokio::test]
async fn custom_certificate_upload_and_revert() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/certificates/custom"))
        .and(body_string_contains("force=1"))
        .and(body_string_contains("restart=1"))
        .respond_with(json(
            r#"{"data":{"filename":"pveproxy-ssl.pem","subject":"CN=pve1.example.com","notafter":1800000000}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/nodes/pve1/certificates/custom"))
        .and(query_param("restart", "1"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let mut params = HashMap::new();
    params.insert(
        "certificates".to_string(),
        "-----BEGIN CERTIFICATE-----".to_string(),
    );
    params.insert("key".to_string(), "-----BEGIN PRIVATE KEY-----".to_string());
    params.insert("force".to_string(), "1".to_string());
    params.insert("restart".to_string(), "1".to_string());
    let info = c.upload_certificate("pve1", &params).await.unwrap();
    assert_eq!(info.subject.as_deref(), Some("CN=pve1.example.com"));

    assert_eq!(
        c.delete_custom_certificate("pve1", true).await.unwrap(),
        None
    );
}

/// Order is a POST, renew a PUT on the same path — mixing them up would order
/// a second certificate instead of renewing the one in place.
#[tokio::test]
async fn acme_order_and_renew_use_different_methods() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/certificates/acme/certificate"))
        .respond_with(json(r#"{"data":"UPID:pve1:acmenewcert"}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/nodes/pve1/certificates/acme/certificate"))
        .and(body_string_contains("force=1"))
        .respond_with(json(r#"{"data":"UPID:pve1:acmerenewcert"}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    assert_eq!(
        c.acme_order_certificate("pve1").await.unwrap(),
        "UPID:pve1:acmenewcert"
    );
    assert_eq!(
        c.acme_renew_certificate("pve1", true).await.unwrap(),
        "UPID:pve1:acmerenewcert"
    );
}

#[tokio::test]
async fn acme_accounts_and_plugins_are_cluster_wide() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/acme/account"))
        .respond_with(json(r#"{"data":[{"name":"default"}]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/acme/account/default"))
        .respond_with(json(
            r#"{"data":{"directory":"https://acme-v02.api.letsencrypt.org/directory",
                "location":"https://acme-v02.api.letsencrypt.org/acme/acct/1",
                "tos":"https://letsencrypt.org/documents/LE-SA-v1.4.pdf",
                "account":{"status":"valid","contact":["mailto:admin@example.com"]}}}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/acme/plugins"))
        .respond_with(json(
            r#"{"data":[{"plugin":"standalone","type":"standalone"},
                {"plugin":"cf","type":"dns","api":"cf","digest":"abc"}]}"#,
        ))
        .mount(&server)
        .await;

    let c = client(&server).await;
    let accounts = c.acme_accounts().await.unwrap();
    assert_eq!(accounts[0].name, "default");

    let detail = c.acme_account("default").await.unwrap();
    assert_eq!(detail["account"]["status"], "valid");

    let plugins = c.acme_plugins().await.unwrap();
    assert_eq!(plugins[1]["api"], "cf");
}

/// A node with no ACME config answers the account listing with an empty list,
/// not an error — the view's "no account configured" hint keys off that.
#[tokio::test]
async fn acme_accounts_can_be_empty() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/cluster/acme/account"))
        .respond_with(json(r#"{"data":[]}"#))
        .mount(&server)
        .await;

    assert!(client(&server)
        .await
        .acme_accounts()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn access_users_and_acl() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/access/users"))
        .respond_with(json(
            r#"{"data":[{"userid":"root@pam","enable":1},{"userid":"alice@pve","comment":"dev","enable":1}]}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api2/json/access/acl"))
        .and(body_string_contains("roles=PVEAuditor"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api2/json/access/users/alice@pve"))
        .respond_with(json(r#"{"data":null}"#))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server).await;
    let users = c.access_users().await.unwrap();
    assert_eq!(users[1].userid, "alice@pve");

    let mut params = HashMap::new();
    params.insert("path".to_string(), "/".to_string());
    params.insert("users".to_string(), "alice@pve".to_string());
    params.insert("roles".to_string(), "PVEAuditor".to_string());
    c.set_acl(&params).await.unwrap();
    c.delete_user("alice@pve").await.unwrap();
}

/// Fixture is the real `/access/permissions` body from PVE 9.2.4 for a token
/// with Privilege Separation off, trimmed to four privileges. The point of the
/// assertions is what is *absent*: PVE lists only paths an ACL names, never
/// `/vms/100`, which is why the frontend walks up the path rather than looking
/// it up directly.
#[tokio::test]
async fn access_permissions_decodes_path_to_privilege_map() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/access/permissions"))
        .and(header("Authorization", format!("PVEAPIToken={TOKEN}")))
        .respond_with(json(
            r#"{"data":{
                "/":{"VM.Backup":1,"Datastore.AllocateSpace":1,"Sys.Audit":1,"VM.Audit":1},
                "/vms":{"VM.Backup":1,"VM.Audit":1},
                "/storage":{"Datastore.AllocateSpace":1}
            }}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let perms = client(&server).await.access_permissions().await.unwrap();
    assert_eq!(perms["/vms"]["VM.Backup"], 1);
    assert_eq!(perms["/storage"]["Datastore.AllocateSpace"], 1);
    assert!(!perms["/vms"].contains_key("Datastore.AllocateSpace"));
    assert!(!perms.contains_key("/vms/100"));
}

/// A token holding nothing gets an empty object and a 200 — not a 403.
/// Confirmed live against PVE 9.2.4 with a privilege-separated token that had
/// no ACLs, and the reason the pre-flight check can run for any connection.
#[tokio::test]
async fn access_permissions_empty_for_a_token_with_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api2/json/access/permissions"))
        .respond_with(json(r#"{"data":{}}"#))
        .mount(&server)
        .await;

    assert!(client(&server)
        .await
        .access_permissions()
        .await
        .unwrap()
        .is_empty());
}

/// The Tasks tab's "start a task" action (#88). PVE answers a bare UPID
/// string, and the request carries no body — the endpoint takes no parameters,
/// which is also why nothing can be upgraded by accident here.
#[tokio::test]
async fn apt_update_posts_and_returns_a_upid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/apt/update"))
        .and(header("Authorization", format!("PVEAPIToken={TOKEN}")))
        .respond_with(json(
            r#"{"data":"UPID:pve1:000C8DCE:005D9D0C:6A6AB670:aptupdate::root@pam:"}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let upid = client(&server).await.apt_update("pve1").await.unwrap();
    assert!(upid.contains(":aptupdate:"));
}

/// A token without Sys.Modify on the node is refused, and the 403 has to reach
/// the frontend intact for `explainError` to name the missing privilege.
#[tokio::test]
async fn apt_update_surfaces_a_permission_failure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve1/apt/update"))
        .respond_with(ResponseTemplate::new(403).set_body_raw(
            r#"{"message":"Permission check failed (/nodes/pve1, Sys.Modify)\n","data":null}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    match client(&server).await.apt_update("pve1").await {
        Err(Error::Api { status, message }) => {
            assert_eq!(status, 403);
            assert!(message.contains("Sys.Modify"));
        }
        other => panic!("expected an Api error, got {other:?}"),
    }
}
