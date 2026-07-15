//! Integration tests against a mocked Proxmox API.
//! No live cluster exists in CI — these verify request shape, auth header,
//! response decoding, and error mapping against recorded fixture bodies.

use std::collections::HashMap;

use proxmox_desktop_lib::proxmox::types::{GuestKind, PowerAction};
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

    let ifaces = client(&server).await.node_network("pve1").await.unwrap();
    assert_eq!(ifaces.len(), 2);
    assert_eq!(ifaces[0].kind, "bridge");
    assert_eq!(ifaces[0].bridge_ports.as_deref(), Some("eno1"));
}
