//! An OpenAI-compatible LLM served *inside* a guest (issue #99).
//!
//! The chat surface is not the interesting part — generic OpenAI-compatible
//! frontends are a crowded, solved problem. **Discovery is**, because pxx-dex
//! is the only client that knows both the guest inventory and the topology
//! between the client machine and that guest.
//!
//! ## Why a guest's endpoint is not simply its IP
//!
//! Measured on the driving case, `lab`'s CT 100 (llama.cpp on `0.0.0.0:8080`,
//! on the port-less NAT bridge `vmbr1`):
//!
//! ```text
//! guest tailnet    100.111.194.35:8080  200
//! node LAN + DNAT  192.168.1.13:8080    200
//! node tailnet     100.117.56.34:8080   fails - the DNAT is on wlo1/vmbr0, not tailscale0
//! Proxmox-visible  10.20.20.10:8080     fails - unroutable off-box
//! ```
//!
//! So the address Proxmox reports is the one candidate that cannot work here,
//! and the node's tailnet address does not inherit the node's DNAT rules. The
//! only robust answer is to try several candidates, remember what answered,
//! and let the user override.
//!
//! The guest's *own* tailnet address is the candidate that generalises best,
//! and #75's `scan::scan_tailscale` already enumerates peers — a guest that
//! joined the tailnet is its own peer, matched here by hostname.
//!
//! (#99's issue text says this resolution is shared with #65 and should be
//! lifted. It cannot be: #65 reaches a guest over SSH-to-the-node plus
//! `pct exec`, and never resolves an address at all. #75 is the donor.)
//!
//! ## Why the HTTP lives here and not in the webview
//!
//! Tauri's CSP is `null`, so the page *could* fetch the endpoint directly, and
//! llama.cpp happens to echo `Access-Control-Allow-Origin` for any origin. That
//! is a property of llama.cpp, not of OpenAI-compatible servers in general —
//! ollama restricts origins unless `OLLAMA_ORIGINS` says otherwise. A Rust
//! client has no CORS to satisfy, so the panel is not silently limited to one
//! server implementation. Do not "simplify" this into the view.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::connections;
use crate::docker;
use crate::host;
use crate::proxmox::types::GuestKind;
use crate::scan;
use crate::ssh::{self, SshSessions};

/// Ports tried when the user has not named one: llama.cpp/llama-server and
/// vLLM's default, ollama's, uvicorn's, and LM Studio's. A short documented
/// list on purpose — scanning ranges across four addresses would turn a tab
/// open into a port scan.
const PROBE_PORTS: [u16; 4] = [8080, 11434, 8000, 1234];

/// One candidate's budget. Long enough for a loaded model on a busy CPU box to
/// answer a metadata request, short enough that a full miss over four
/// addresses is still a few seconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Candidates in flight at once. The list is at most ~20 entries, so this is
/// about not opening twenty sockets at a tab open, not about throughput.
const PROBE_CONCURRENCY: usize = 8;

const CACHE_FILE: &str = "llm_endpoints.json";

/// Where the guest keeps the compose project that runs the server (#100).
///
/// Switching model is **not an API call** — `llama-server` serves exactly one
/// model per process, and there is no endpoint that loads a different one. So
/// it is a config edit plus a restart, and that means a convention.
///
/// The one pxx-dex drives is a `docker compose` project whose `.env` names the
/// model: `MODEL=` picks the file and `ALIAS=` is the id `/v1/models` then
/// reports. #100's issue text assumes a different convention (an `llm-use`
/// script) — that script does not exist on the box, on the node or in the
/// guest, though `.env` still carries a comment recommending it.
///
/// A guest without this layout gets a sentence saying so and nothing else
/// happens. Inventing a second mechanism to guess at would be worse.
const LLM_COMPOSE_DIR: &str = "/opt/llm";

/// Directories searched inside the guest for model files.
///
/// ponytail: a short list, like `PROBE_PORTS`. `/props` reports a `model_path`,
/// but it is the path *inside the container* (`/models/...` on the real box,
/// bind-mounted from `/opt/models`), so it cannot be listed over the guest
/// channel. A box keeping models somewhere else needs a line added here.
const MODEL_DIRS: [&str; 3] = ["/opt/models", "/models", "/var/lib/models"];

/// The largest share of the guest's RAM a model file may take.
///
/// The weights are not the whole cost: the KV cache, the compute buffers and
/// the server itself all have to fit alongside them. The issue's example is a
/// 16 GiB model in a 13 GiB container, which does not fail cleanly — it
/// OOM-loops. On the real box the loaded 10.1 GiB model sits at 0.78 of 13.0
/// GiB and runs, so the line is above that and below 1.0.
const MAX_MODEL_SHARE_OF_RAM: f64 = 0.85;

/// A guest's LLM endpoint, as found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmEndpoint {
    /// Scheme, host and port, no trailing slash — `http://100.111.194.35:8080`.
    pub base_url: String,
    /// Model ids from `/v1/models`. For llama-server this is one entry, the
    /// only model the process serves (which is why switching it is #100).
    pub models: Vec<String>,
    /// True when this came from the user's manual override rather than a probe.
    pub manual: bool,
}

/// One model file on the guest's disk, and whether it can actually be used.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    /// Bare filename — what goes into `MODEL=`.
    pub file: String,
    /// Full path inside the guest, so the view can say where it looked.
    pub path: String,
    pub bytes: u64,
    /// The model the server is serving right now.
    pub loaded: bool,
    /// Whether it fits in the guest's RAM with room for the KV cache.
    pub fits: bool,
}

/// One message in the conversation, in the shape the API expects.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// One streamed piece of a reply. `done` arrives exactly once per request,
/// including when the stream failed — the view re-enables its input on it, so
/// a missing `done` would leave the panel stuck.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatChunk {
    pub delta: String,
    pub done: bool,
    /// Set when generation stopped because of an error rather than the model
    /// finishing. The partial reply so far is kept either way.
    pub error: Option<String>,
}

/// Request ids the user has cancelled. A flag checked between chunks rather
/// than an `AbortHandle`: the stream loop is the only reader, so there is no
/// task lifetime to juggle.
#[derive(Default)]
pub struct LlmCancels(pub Mutex<HashSet<String>>);

/// What is remembered per guest, on disk.
///
/// `manual` is the difference between "the user told us where this is" and
/// "this is what answered last time". A manual entry is never raced against
/// other candidates; a remembered one is only tried *first*, so a guest that
/// moved is re-found instead of being permanently wrong.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedEndpoint {
    url: String,
    #[serde(default)]
    manual: bool,
}

fn cache_key(connection_id: &str, kind: GuestKind, vmid: u32) -> String {
    format!("{connection_id}/{}/{vmid}", kind.as_path())
}

fn cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(CACHE_FILE))
}

/// A URL is not a secret, so this is a plain file next to the connection
/// profiles rather than a keyring entry — and it is per-guest, so it does not
/// belong on `ConnectionInfo` either.
fn read_cache(app: &tauri::AppHandle) -> HashMap<String, CachedEndpoint> {
    let Ok(path) = cache_path(app) else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_cache(
    app: &tauri::AppHandle,
    cache: &HashMap<String, CachedEndpoint>,
) -> Result<(), String> {
    let path = cache_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(path, raw).map_err(|e| e.to_string())
}

/// Strip a trailing slash so `{base}/v1/models` never becomes `//v1/models`,
/// and default a bare host to `http://`.
fn normalise_base(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

/// The candidate base URLs to try, in priority order.
///
/// A manual override collapses the list to itself: the user has said where the
/// endpoint is, and quietly probing somewhere else after they said so is worse
/// than reporting that their address did not answer.
///
/// Otherwise: whatever answered last time, then the guest's own address, then
/// its tailnet address, then the node. Addresses are grouped before ports so a
/// specific address is exhausted before a less specific one wins on a lucky
/// port.
///
/// **The node comes last, and only ever paired with a port the guest is
/// actually listening on.** A node address is not guest-specific:
/// `192.168.1.13:8080` answers identically for every guest on that node, so
/// probing it with the default port list handed *every* guest on `lab` an LLM
/// tab pointing at CT 100's model — found the first time this ran against the
/// live box. Requiring the guest to serve the port ties the hit back to this
/// guest.
///
/// ponytail: a forward that also *renames* the port (node 9999 -> guest 8080)
/// still cannot be attributed to a guest, so it is not tried. That is what the
/// manual override is for.
fn candidate_urls(
    remembered: Option<&CachedEndpoint>,
    guest_ip: Option<&str>,
    tailnet_ip: Option<&str>,
    node_host: Option<&str>,
    guest_ports: &[u16],
    port_hint: Option<u16>,
) -> Vec<String> {
    if let Some(entry) = remembered.filter(|e| e.manual) {
        return vec![normalise_base(&entry.url)];
    }

    // What the guest itself serves goes first: that is evidence, where the
    // default list is a guess.
    let mut ports: Vec<u16> = port_hint.into_iter().collect();
    ports.extend(guest_ports.iter().copied());
    ports.extend(PROBE_PORTS);

    let mut out: Vec<String> = remembered
        .map(|e| normalise_base(&e.url))
        .into_iter()
        .collect();
    let mut push = |addr: Option<&str>, ports: &[u16]| {
        let Some(addr) = addr.map(str::trim).filter(|a| !a.is_empty()) else {
            return;
        };
        for port in ports {
            out.push(format!("http://{addr}:{port}"));
        }
    };
    push(guest_ip, &ports);
    push(tailnet_ip, &ports);
    push(node_host, guest_ports);

    let mut seen = HashSet::new();
    out.retain(|url| seen.insert(url.clone()));
    out
}

/// The IPv4 address an LXC is configured with, from `pct config`.
///
/// `net0: name=eth0,bridge=vmbr1,...,ip=10.20.20.10/24,type=veth`. A guest set
/// to `ip=dhcp` (or `manual`) has no address here at all — that is a miss, not
/// an error, and the other candidates still apply.
fn lxc_ip(config: &str) -> Option<String> {
    config
        .lines()
        .filter(|line| line.trim_start().starts_with("net"))
        .find_map(|line| {
            line.split(',')
                .find_map(|field| field.trim().strip_prefix("ip="))
                .map(|v| v.split('/').next().unwrap_or(v).trim().to_string())
        })
        .filter(|ip| !ip.is_empty() && ip != "dhcp" && ip != "manual" && ip != "auto")
}

/// The first non-loopback IPv4 a VM's guest agent reports, from
/// `qm guest cmd {vmid} network-get-interfaces`.
fn qemu_ip(agent_json: &str) -> Option<String> {
    let ifaces: serde_json::Value = serde_json::from_str(agent_json).ok()?;
    ifaces.as_array()?.iter().find_map(|iface| {
        iface
            .get("ip-addresses")?
            .as_array()?
            .iter()
            .find_map(|entry| {
                let ip = entry.get("ip-address")?.as_str()?;
                let is_v4 = entry
                    .get("ip-address-type")
                    .and_then(|t| t.as_str())
                    .is_none_or(|t| t == "ipv4");
                (is_v4 && !ip.starts_with("127.")).then(|| ip.to_string())
            })
    })
}

/// `GET {base}/v1/models` — a 200 with a `data` array is the cheap,
/// unambiguous "this speaks the OpenAI API" signal.
async fn probe_models(client: &reqwest::Client, base: &str) -> Option<Vec<String>> {
    let resp = client.get(format!("{base}/v1/models")).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    let data = body.get("data")?.as_array()?;
    Some(
        data.iter()
            .filter_map(|m| Some(m.get("id")?.as_str()?.to_string()))
            .collect(),
    )
}

/// The first candidate that answers, in list order.
///
/// `buffered` rather than `buffer_unordered`: several candidates can be live at
/// once (both the tailnet and the DNAT path answer on the driving case), so the
/// winner has to be decided by priority, not by which socket was quicker.
pub async fn first_endpoint(
    client: &reqwest::Client,
    candidates: Vec<String>,
) -> Option<LlmEndpoint> {
    let mut probes = Box::pin(
        stream::iter(candidates)
            .map(|base| {
                let client = client.clone();
                async move {
                    probe_models(&client, &base)
                        .await
                        .map(|models| LlmEndpoint {
                            base_url: base,
                            models,
                            manual: false,
                        })
                }
            })
            .buffered(PROBE_CONCURRENCY),
    );
    while let Some(hit) = probes.next().await {
        if hit.is_some() {
            return hit;
        }
    }
    None
}

fn probe_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

/// The guest's configured address, best effort.
///
/// Every failure here is a miss rather than an error: no SSH configured, guest
/// powered off, no qemu-guest-agent, a node that is not a PVE host at all. The
/// remaining candidates do not depend on it.
async fn guest_address(
    app: &tauri::AppHandle,
    sessions: &SshSessions,
    connection_id: &str,
    kind: GuestKind,
    vmid: u32,
) -> Option<String> {
    let command = match kind {
        GuestKind::Lxc => format!("pct config {vmid}"),
        GuestKind::Qemu => format!("qm guest cmd {vmid} network-get-interfaces"),
    };
    let out = ssh::exec_on_connection(app, sessions, connection_id, &command)
        .await
        .ok()?;
    if out.exit_status != 0 {
        return None;
    }
    match kind {
        GuestKind::Lxc => lxc_ip(&out.stdout),
        GuestKind::Qemu => qemu_ip(&out.stdout),
    }
}

/// The TCP ports the guest is listening on, from `ss` run inside it.
///
/// This is what makes a node-side address attributable to *this* guest, and it
/// narrows the port list from a guess to a fact. Reuses #104's `ss` command and
/// parser rather than growing a second one — they have already drifted apart
/// once (#112), which is why they are shared constants now.
///
/// Empty on any failure: a guest with no `ss`, no SSH to the node, or one that
/// is powered off simply contributes no ports.
async fn guest_listening_ports(
    app: &tauri::AppHandle,
    sessions: &SshSessions,
    connection_id: &str,
    kind: GuestKind,
    vmid: u32,
) -> Vec<u16> {
    let Ok(out) =
        docker::exec_in_guest(app, sessions, connection_id, kind, vmid, host::SS_LISTENING).await
    else {
        return Vec::new();
    };
    if out.exit_status != 0 {
        return Vec::new();
    }
    let mut ports: Vec<u16> = host::parse_ports(&out.stdout)
        .into_iter()
        .filter(|p| p.proto.starts_with("tcp"))
        .map(|p| p.port)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// A guest that joined the tailnet is its own peer, matched by hostname —
/// which is what `pct config`'s `hostname:` and the Proxmox guest name are.
async fn tailnet_address(guest_name: &str) -> Option<String> {
    if guest_name.is_empty() {
        return None;
    }
    let peers = scan::scan_tailscale().await.ok()?;
    peers
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(guest_name))
        .map(|p| p.ip)
}

/// Find this guest's OpenAI-compatible endpoint, or `Ok(None)` if it has none.
///
/// `Ok(None)` rather than an error because "this guest does not serve an LLM"
/// is the normal case for almost every guest — the caller hides the tab, the
/// same way `docker_ps` doubles as the Docker probe (#65).
#[tauri::command]
pub async fn llm_probe(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    guest_name: String,
    port_hint: Option<u16>,
) -> Result<Option<LlmEndpoint>, String> {
    let key = cache_key(&connection_id, kind, vmid);
    let mut cache = read_cache(&app);
    let remembered = cache.get(&key).cloned();

    let manual = remembered.as_ref().is_some_and(|e| e.manual);
    let (guest_ip, tailnet_ip, node_host, guest_ports) = if manual {
        // The list collapses to the override anyway; skip two SSH round trips
        // and a `tailscale status` for candidates that will not be used.
        (None, None, None, Vec::new())
    } else {
        let guest_ip = guest_address(&app, &sessions, &connection_id, kind, vmid).await;
        let guest_ports = guest_listening_ports(&app, &sessions, &connection_id, kind, vmid).await;
        let tailnet_ip = tailnet_address(&guest_name).await;
        let node_host = connections::ssh_target(&app, &connection_id)
            .ok()
            .map(|t| t.host);
        (guest_ip, tailnet_ip, node_host, guest_ports)
    };

    let candidates = candidate_urls(
        remembered.as_ref(),
        guest_ip.as_deref(),
        tailnet_ip.as_deref(),
        node_host.as_deref(),
        &guest_ports,
        port_hint,
    );

    let Some(mut found) = first_endpoint(&probe_client()?, candidates).await else {
        return Ok(None);
    };
    found.manual = manual;

    if !manual {
        cache.insert(
            key,
            CachedEndpoint {
                url: found.base_url.clone(),
                manual: false,
            },
        );
        // A cache that cannot be written costs one re-probe next time; it is
        // not a reason to fail a probe that just succeeded.
        let _ = write_cache(&app, &cache);
    }
    Ok(Some(found))
}

/// Set or clear the manual endpoint override for a guest.
#[tauri::command]
pub async fn llm_set_endpoint(
    app: tauri::AppHandle,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    base_url: Option<String>,
) -> Result<(), String> {
    let key = cache_key(&connection_id, kind, vmid);
    let mut cache = read_cache(&app);
    match base_url.map(|u| normalise_base(&u)) {
        Some(url) if !url.is_empty() && url != "http://" => {
            cache.insert(key, CachedEndpoint { url, manual: true });
        }
        // Clearing drops the remembered address too, so the next probe starts
        // from scratch rather than preferring whatever the override replaced.
        _ => {
            cache.remove(&key);
        }
    }
    write_cache(&app, &cache)
}

/// Pull complete SSE `data:` payloads out of a growing buffer.
///
/// Returns the deltas found and whether the terminating `[DONE]` was seen. A
/// partial trailing line stays in `buf` — chunk boundaries fall mid-line often
/// enough that treating the buffer as whole lines is the whole trick here.
fn take_sse_deltas(buf: &mut String) -> (Vec<String>, bool) {
    let mut deltas = Vec::new();
    let mut done = false;
    let Some(last_newline) = buf.rfind('\n') else {
        return (deltas, done);
    };
    let complete: String = buf.drain(..=last_newline).collect();

    for line in complete.lines() {
        let Some(payload) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload == "[DONE]" {
            done = true;
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        if let Some(text) = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
            .and_then(|d| d.get("content"))
            .and_then(|t| t.as_str())
        {
            deltas.push(text.to_string());
        }
    }
    (deltas, done)
}

/// Stream one completion, handing each delta to `on_chunk`.
///
/// Split out of the command so the wiremock tests can drive it without a Tauri
/// app: `cancelled` is polled between chunks and `on_chunk` receives exactly
/// one `done` chunk, whatever happened.
pub async fn stream_chat<F>(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    messages: &[ChatMessage],
    cancelled: &(dyn Fn() -> bool + Sync),
    mut on_chunk: F,
) where
    F: FnMut(ChatChunk),
{
    let finish = |on_chunk: &mut F, error: Option<String>| {
        on_chunk(ChatChunk {
            delta: String::new(),
            done: true,
            error,
        });
    };

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });
    let resp = match client
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return finish(&mut on_chunk, Some(e.to_string())),
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        let detail = detail.trim();
        let message = if detail.is_empty() {
            format!("the endpoint answered {status}")
        } else {
            format!("the endpoint answered {status}: {detail}")
        };
        return finish(&mut on_chunk, Some(message));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(next) = stream.next().await {
        if cancelled() {
            return finish(&mut on_chunk, None);
        }
        match next {
            Ok(bytes) => buf.push_str(&String::from_utf8_lossy(&bytes)),
            Err(e) => return finish(&mut on_chunk, Some(e.to_string())),
        }
        let (deltas, done) = take_sse_deltas(&mut buf);
        for delta in deltas {
            on_chunk(ChatChunk {
                delta,
                done: false,
                error: None,
            });
        }
        if done {
            return finish(&mut on_chunk, None);
        }
    }
    // The connection ended without `[DONE]` -- a guest that stopped serving
    // mid-generation. The partial reply stays on screen; the view re-probes
    // before the next send.
    finish(
        &mut on_chunk,
        Some("the endpoint stopped responding mid-reply".to_string()),
    );
}

/// Stream a chat completion, one `ChatChunk` per `on_chunk` message.
///
/// `tauri::ipc::Channel` rather than a websocket bridge: the terminal needs a
/// bridge because it speaks pve-xtermjs's wire protocol, and there is no such
/// constraint here.
#[tauri::command]
pub async fn llm_chat(
    cancels: tauri::State<'_, LlmCancels>,
    base_url: String,
    model: String,
    messages: Vec<ChatMessage>,
    request_id: String,
    on_chunk: Channel<ChatChunk>,
) -> Result<(), String> {
    // No read timeout: a CPU-inference box can take tens of seconds to emit
    // the first token of a long prompt, and cancelling is the user's job.
    let client = reqwest::Client::builder()
        .connect_timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    let is_cancelled = || cancels.0.lock().unwrap().contains(&request_id);
    stream_chat(
        &client,
        &normalise_base(&base_url),
        &model,
        &messages,
        &is_cancelled,
        |chunk| {
            let _ = on_chunk.send(chunk);
        },
    )
    .await;

    cancels.0.lock().unwrap().remove(&request_id);
    Ok(())
}

/// Ask a running `llm_chat` to stop. Silent if the id is unknown — a cancel
/// racing the last chunk is normal, not an error worth showing.
#[tauri::command]
pub fn llm_cancel(cancels: tauri::State<'_, LlmCancels>, request_id: String) {
    cancels.0.lock().unwrap().insert(request_id);
}

/// `MemTotal` from `/proc/meminfo`, in bytes.
///
/// The guest's own view, not the Proxmox `maxmem`: a container's cgroup limit
/// is what the loader actually runs into, and it is one line to read over the
/// channel already open.
fn parse_mem_total(meminfo: &str) -> Option<u64> {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// Model files out of `ls -l --block-size=1`, as `(path, bytes)`.
///
/// `--block-size=1` because the default `ls -l` size is already bytes on GNU
/// coreutils but not everywhere; asking explicitly costs nothing and removes
/// the doubt. Lines that are not files (`total 123`, an error on a missing
/// directory) do not parse and are dropped.
fn parse_gguf_listing(stdout: &str) -> Vec<(String, u64)> {
    stdout
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // perms links owner group size month day time path
            let bytes = fields.get(4)?.parse::<u64>().ok()?;
            let path = fields.get(8..)?.join(" ");
            path.ends_with(".gguf").then_some((path, bytes))
        })
        .collect()
}

/// Whether a model of this size leaves room to actually serve.
fn fits_in_ram(bytes: u64, ram_bytes: u64) -> bool {
    ram_bytes > 0 && (bytes as f64) <= (ram_bytes as f64) * MAX_MODEL_SHARE_OF_RAM
}

/// The `ALIAS=` to write alongside a new `MODEL=`.
///
/// The alias is the id `/v1/models` reports, so leaving the old one in place
/// would have the server announce the previous model's name for the new one.
/// The file's stem, lowercased, is not the hand-picked alias a human would
/// choose — but it is unambiguous and it is honest about what is loaded.
fn alias_for(file: &str) -> String {
    file.trim_end_matches(".gguf").to_ascii_lowercase()
}

/// A model filename safe to interpolate into a shell command and a `sed`
/// replacement. Deliberately strict: this string is written into a file that
/// decides what the server executes.
fn valid_model_file(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 255
        && file.ends_with(".gguf")
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

/// The model currently being served, from `/props`.
///
/// `model_path` is the path inside the *container*, so only its filename is
/// comparable with what the guest's directories hold.
async fn loaded_model_file(client: &reqwest::Client, base_url: &str) -> Option<String> {
    let resp = client
        .get(format!("{base_url}/props"))
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;
    let path = resp.get("model_path")?.as_str()?;
    Some(path.rsplit(['/', '\\']).next()?.to_string())
}

/// Every model file on the guest's disk, with size, fit, and which one is live.
///
/// `Ok(None)` means no `.gguf` was found in any known directory — the panel
/// hides the switcher rather than showing an empty table, the same shape as
/// every other probe here.
#[tauri::command]
pub async fn llm_models_available(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    base_url: Option<String>,
) -> Result<Option<Vec<ModelFile>>, String> {
    let globs = MODEL_DIRS
        .iter()
        .map(|d| format!("{d}/*.gguf"))
        .collect::<Vec<_>>()
        .join(" ");
    let listing = docker::exec_in_guest(
        &app,
        &sessions,
        &connection_id,
        kind,
        vmid,
        &format!("ls -l --block-size=1 {globs} 2>/dev/null; head -1 /proc/meminfo"),
    )
    .await?;

    let ram = parse_mem_total(&listing.stdout).unwrap_or(0);
    let mut files = parse_gguf_listing(&listing.stdout);
    if files.is_empty() {
        return Ok(None);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let loaded = match base_url {
        Some(url) => loaded_model_file(&probe_client()?, &normalise_base(&url)).await,
        None => None,
    };

    Ok(Some(
        files
            .into_iter()
            .map(|(path, bytes)| {
                let file = path.rsplit('/').next().unwrap_or(&path).to_string();
                ModelFile {
                    loaded: loaded.as_deref() == Some(file.as_str()),
                    fits: fits_in_ram(bytes, ram),
                    file,
                    path,
                    bytes,
                }
            })
            .collect(),
    ))
}

/// Point the guest's compose project at a different model file and restart it.
///
/// Returns as soon as the restart is *issued*. The endpoint is then down for
/// roughly a minute while the model loads, which is expected rather than an
/// error — the caller polls [`llm_health`] and re-enables its chat on `ok`.
#[tauri::command]
pub async fn llm_switch_model(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
    kind: GuestKind,
    vmid: u32,
    file: String,
) -> Result<(), String> {
    if !valid_model_file(&file) {
        return Err(format!(
            "{file} is not a model filename this can switch to."
        ));
    }
    let alias = alias_for(&file);

    // Distinct exit codes rather than parsing stderr: each one is a different
    // sentence to the user, and only the last is a real failure to report raw.
    let script = format!(
        "cd {dir} 2>/dev/null || exit 90; \
         test -f .env || exit 91; \
         sed -i 's|^MODEL=.*|MODEL={file}|; s|^ALIAS=.*|ALIAS={alias}|' .env || exit 92; \
         grep -qx 'MODEL={file}' .env || exit 93; \
         docker compose up -d",
        dir = LLM_COMPOSE_DIR,
    );
    let out = docker::exec_in_guest(&app, &sessions, &connection_id, kind, vmid, &script).await?;

    match out.exit_status {
        0 => Ok(()),
        90 | 91 => Err(format!(
            "This guest has no {LLM_COMPOSE_DIR}/.env, so pxx-dex does not know how it starts its \
             server and will not guess. Switch the model on the guest and the panel will pick up \
             the change."
        )),
        92 | 93 => Err(format!(
            "{LLM_COMPOSE_DIR}/.env has no MODEL= line to change. Add one naming the current \
             model file, and this can drive it."
        )),
        _ => Err(format!(
            "The restart failed: {}",
            if out.stderr.trim().is_empty() {
                out.stdout.trim()
            } else {
                out.stderr.trim()
            }
        )),
    }
}

/// Whether the endpoint is serving again. Used to watch a model reload, where
/// "not answering" is the expected state for about a minute, not a failure.
#[tauri::command]
pub async fn llm_health(base_url: String) -> Result<bool, String> {
    let client = probe_client()?;
    let Ok(resp) = client
        .get(format!("{}/health", normalise_base(&base_url)))
        .send()
        .await
    else {
        return Ok(false);
    };
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `pct config 100` on `lab`, the driving case: the guest is
    /// on a port-less NAT bridge, so this address is real and unroutable.
    const PCT_CONFIG: &str = "arch: amd64\n\
cores: 20\n\
hostname: llm\n\
memory: 13312\n\
net0: name=eth0,bridge=vmbr1,firewall=0,gw=10.20.20.1,hwaddr=BC:24:11:0B:A6:21,ip=10.20.20.10/24,type=veth\n\
onboot: 1\n\
rootfs: local-lvm:vm-100-disk-0,size=65G\n";

    #[test]
    fn lxc_ip_reads_the_net_line() {
        assert_eq!(lxc_ip(PCT_CONFIG), Some("10.20.20.10".to_string()));
    }

    /// A DHCP guest has no address in its config at all. That is a miss, and
    /// the node and tailnet candidates still have to be produced.
    #[test]
    fn lxc_ip_is_none_for_dhcp() {
        let config = "net0: name=eth0,bridge=vmbr0,ip=dhcp,type=veth\n";
        assert_eq!(lxc_ip(config), None);
    }

    #[test]
    fn qemu_ip_skips_loopback() {
        let json = r#"[
            {"name":"lo","ip-addresses":[{"ip-address-type":"ipv4","ip-address":"127.0.0.1"}]},
            {"name":"ens18","ip-addresses":[
                {"ip-address-type":"ipv6","ip-address":"fe80::1"},
                {"ip-address-type":"ipv4","ip-address":"192.168.1.50"}]}
        ]"#;
        assert_eq!(qemu_ip(json), Some("192.168.1.50".to_string()));
    }

    #[test]
    fn candidates_cover_the_guest_addresses_on_every_port() {
        let urls = candidate_urls(
            None,
            Some("10.20.20.10"),
            Some("100.111.194.35"),
            Some("192.168.1.13"),
            &[8080],
            None,
        );
        assert_eq!(urls[0], "http://10.20.20.10:8080");
        // Addresses are grouped: the guest's own ports are exhausted before its
        // tailnet address is tried at all.
        assert!(urls[..PROBE_PORTS.len()]
            .iter()
            .all(|u| u.starts_with("http://10.20.20.10:")));
        assert!(urls.contains(&"http://100.111.194.35:8080".to_string()));
        // The node comes last, and only on the port the guest actually serves.
        assert_eq!(urls.last().unwrap(), "http://192.168.1.13:8080");
        assert!(!urls.iter().any(|u| u == "http://192.168.1.13:11434"));
    }

    /// The bug the first live run found: a node address answers the same for
    /// every guest on that node, so probing it blindly gave every guest on
    /// `lab` an LLM tab pointing at CT 100's model. A guest we cannot see
    /// inside contributes no ports, and so contributes no node candidate.
    #[test]
    fn the_node_is_not_probed_for_a_guest_that_serves_nothing() {
        let urls = candidate_urls(None, None, None, Some("192.168.1.13"), &[], None);
        assert!(urls.is_empty(), "{urls:?}");
    }

    #[test]
    fn port_hint_is_tried_before_the_defaults() {
        let urls = candidate_urls(None, Some("10.20.20.10"), None, None, &[], Some(9999));
        assert_eq!(urls[0], "http://10.20.20.10:9999");
        assert_eq!(urls[1], "http://10.20.20.10:8080");
    }

    /// A port the guest is really listening on beats the default list: it is
    /// evidence where the defaults are a guess.
    #[test]
    fn a_served_port_is_tried_before_the_defaults() {
        let urls = candidate_urls(None, Some("10.20.20.10"), None, None, &[3000], None);
        assert_eq!(urls[0], "http://10.20.20.10:3000");
        assert_eq!(urls[1], "http://10.20.20.10:8080");
    }

    /// A manual override is the user telling us where the endpoint is. Probing
    /// anywhere else after that is worse than reporting that it did not answer.
    #[test]
    fn manual_override_is_the_only_candidate() {
        let manual = CachedEndpoint {
            url: "http://10.0.0.9:1234/".into(),
            manual: true,
        };
        let urls = candidate_urls(
            Some(&manual),
            Some("10.20.20.10"),
            Some("100.111.194.35"),
            Some("192.168.1.13"),
            &[8080],
            None,
        );
        assert_eq!(urls, vec!["http://10.0.0.9:1234"]);
    }

    /// A remembered winner is only a head start. A guest that moved has to be
    /// re-found, so the rest of the list must still be there behind it.
    #[test]
    fn remembered_endpoint_is_first_but_not_exclusive() {
        let remembered = CachedEndpoint {
            url: "http://192.168.1.13:8080".into(),
            manual: false,
        };
        let urls = candidate_urls(
            Some(&remembered),
            Some("10.20.20.10"),
            None,
            Some("192.168.1.13"),
            &[8080],
            None,
        );
        assert_eq!(urls[0], "http://192.168.1.13:8080");
        assert!(urls.len() > 1);
        // ...and it is not repeated when the same address comes up again.
        assert_eq!(
            urls.iter()
                .filter(|u| *u == "http://192.168.1.13:8080")
                .count(),
            1
        );
    }

    #[test]
    fn bare_host_gets_a_scheme_and_loses_the_trailing_slash() {
        assert_eq!(
            normalise_base("192.168.1.13:8080/"),
            "http://192.168.1.13:8080"
        );
        assert_eq!(normalise_base("https://box:8080"), "https://box:8080");
    }

    /// Captured from `lab` CT 100: the four models on disk and the guest's own
    /// `MemTotal`, in the single command `llm_models_available` runs.
    const MODEL_LISTING: &str = "-rw-r--r-- 1 root root   639447744 Aug  1 09:17 /opt/models/Qwen3-0.6B-Q8_0.gguf\n\
-rw-r--r-- 1 root root  9159818624 Aug  1 09:16 /opt/models/Qwen3-14B-UD-Q4_K_XL.gguf\n\
-rw-r--r-- 1 root root 10845131168 Aug  1 09:39 /opt/models/Qwen3-30B-A3B-Instruct-2507-UD-IQ2_M.gguf\n\
-rw-r--r-- 1 root root  2546340960 Aug  1 09:40 /opt/models/Qwen3-4B-Instruct-2507-UD-Q4_K_XL.gguf\n\
MemTotal:       13631488 kB\n";

    #[test]
    fn model_listing_reads_paths_and_byte_sizes() {
        let files = parse_gguf_listing(MODEL_LISTING);
        assert_eq!(files.len(), 4);
        assert_eq!(
            files[2],
            (
                "/opt/models/Qwen3-30B-A3B-Instruct-2507-UD-IQ2_M.gguf".to_string(),
                10_845_131_168
            )
        );
    }

    /// The `MemTotal` line rides in the same output as the listing and must not
    /// be mistaken for a file.
    #[test]
    fn model_listing_ignores_everything_that_is_not_a_gguf() {
        let noisy = format!("total 23190765568\n{MODEL_LISTING}ls: cannot access '/models/*.gguf': No such file or directory\n");
        assert_eq!(parse_gguf_listing(&noisy).len(), 4);
    }

    #[test]
    fn mem_total_is_read_in_bytes() {
        assert_eq!(parse_mem_total(MODEL_LISTING), Some(13_631_488 * 1024));
        assert_eq!(parse_mem_total("no meminfo here"), None);
    }

    /// The real box: 13.0 GiB of RAM serving a 10.1 GiB model. That has to pass,
    /// and the issue's 16 GiB-in-13 GiB example has to fail — it does not error
    /// on the guest, it OOM-loops.
    #[test]
    fn ram_guard_admits_the_live_model_and_refuses_an_oversized_one() {
        let ram = 13_631_488u64 * 1024;
        assert!(fits_in_ram(10_845_131_168, ram));
        assert!(!fits_in_ram(16 * 1024 * 1024 * 1024, ram));
        // A guest whose RAM could not be read refuses everything rather than
        // waving through a model that may not fit.
        assert!(!fits_in_ram(1, 0));
    }

    #[test]
    fn alias_follows_the_file_so_v1_models_stops_lying() {
        assert_eq!(
            alias_for("Qwen3-14B-UD-Q4_K_XL.gguf"),
            "qwen3-14b-ud-q4_k_xl"
        );
    }

    /// This string is written into the file that decides what the server runs,
    /// so anything that could break out of the `sed` replacement or the shell
    /// has to be refused rather than escaped.
    #[test]
    fn model_filenames_with_shell_or_sed_metacharacters_are_refused() {
        assert!(valid_model_file("Qwen3-0.6B-Q8_0.gguf"));
        assert!(!valid_model_file("../../etc/passwd.gguf"));
        assert!(!valid_model_file("a|b.gguf"));
        assert!(!valid_model_file("a b.gguf"));
        assert!(!valid_model_file("$(reboot).gguf"));
        assert!(!valid_model_file("model.bin"));
        assert!(!valid_model_file(""));
    }

    #[test]
    fn sse_deltas_are_joined_across_chunk_boundaries() {
        let mut buf = String::new();
        buf.push_str("data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\ndata: {\"choi");
        let (first, done) = take_sse_deltas(&mut buf);
        assert_eq!(first, vec!["Hel"]);
        assert!(!done);
        // The half line stayed behind and completes with the next network read.
        buf.push_str("ces\":[{\"delta\":{\"content\":\"lo\"}}]}\ndata: [DONE]\n");
        let (second, done) = take_sse_deltas(&mut buf);
        assert_eq!(second, vec!["lo"]);
        assert!(done);
    }

    /// llama.cpp sends keep-alive comment lines and a role-only first delta;
    /// neither is text and neither may show up in the reply.
    #[test]
    fn sse_ignores_comments_and_contentless_deltas() {
        let mut buf = String::from(
            ": ping\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n",
        );
        let (deltas, done) = take_sse_deltas(&mut buf);
        assert_eq!(deltas, vec!["hi"]);
        assert!(!done);
    }
}
