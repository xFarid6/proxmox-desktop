//! Local websocket bridge for an SSH shell -- the SSH-backed sibling of
//! `console.rs::open_console`. Speaks the same pve-xtermjs wire protocol
//! `ConsoleView.vue` already implements for the VNC/term console, so the
//! frontend terminal needs zero changes to attach to an SSH shell instead
//! of a Proxmox termproxy.
//!
//! Frame table (client -> bridge): `{user}:{ticket}\n` once on connect, then
//! `0:{byteLen}:{data}` keystrokes, `1:{cols}:{rows}:` resize, `2` keepalive
//! (dropped). Bridge -> client is just the raw shell output bytes.
//!
//! Unlike `console.rs`, the auth line here is *checked*. There, the ticket is
//! validated by the remote Proxmox endpoint, so the bridge can pass it through
//! blindly. Here the SSH session is already authenticated from the keyring
//! before the listener even binds, so nothing downstream checks anything --
//! this socket is the only thing between a local process and a root shell on
//! the node. So the ticket is a one-time nonce this module generates, and a
//! client that can't produce it gets dropped.

use std::path::Path;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use russh::ChannelMsg;
use tauri::Manager;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use crate::connections;
use crate::console::ConsoleInfo;
use crate::ssh::{self, Auth, Session, SshSessions};

/// Initial PTY size. Purely a starting point -- `ConsoleView.vue` sends a
/// `1:{cols}:{rows}:` resize frame right after connecting (see its
/// `attachTerm`), which corrects it to the real terminal size immediately.
const INITIAL_COLS: u16 = 80;
const INITIAL_ROWS: u16 = 24;

/// How long the bridge waits for the frontend to attach before giving up.
/// The window between returning the port and the webview dialing it is
/// milliseconds; this only has to be generous enough not to race a slow
/// machine, and short enough that an unclaimed shell doesn't linger.
const ACCEPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// One-time ticket gating the local bridge socket, as lowercase hex.
///
/// Uses rustls' provider RNG rather than pulling in a `rand` dependency --
/// `console.rs` already reaches for the same provider for its TLS config.
fn bridge_ticket() -> Result<String, String> {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let mut bytes = [0u8; 32];
    provider
        .secure_random
        .fill(&mut bytes)
        .map_err(|_| "failed to generate a console ticket".to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// One parsed client -> bridge frame off the pve-xtermjs wire protocol.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Keepalive,
}

/// Result of trying to parse one frame off the front of a buffer.
enum Attempt {
    /// A complete frame, and how many bytes of the buffer it consumed.
    Frame(usize, Frame),
    /// Not enough bytes yet to tell -- wait for more.
    Incomplete,
    /// Not a valid frame at the front of the buffer; drop this many bytes
    /// and try again. Keeps a corrupted/unexpected byte from wedging the
    /// parser or panicking on it.
    Skip(usize),
}

fn is_digits(b: &[u8]) -> bool {
    !b.is_empty() && b.iter().all(u8::is_ascii_digit)
}

fn parse_uint(b: &[u8]) -> Option<u32> {
    std::str::from_utf8(b).ok()?.parse().ok()
}

/// Parses `0:{byteLen}:{data}` at the front of `buf`. `byteLen` counts raw
/// bytes, not chars -- `data` is sliced out verbatim, never re-measured
/// after any UTF-8 decoding.
fn parse_data(buf: &[u8]) -> Attempt {
    if buf.len() < 2 {
        return Attempt::Incomplete;
    }
    if buf[1] != b':' {
        return Attempt::Skip(1);
    }
    let rest = &buf[2..];
    let Some(colon) = rest.iter().position(|&b| b == b':') else {
        return if rest.iter().all(u8::is_ascii_digit) {
            Attempt::Incomplete
        } else {
            Attempt::Skip(1)
        };
    };
    let len_bytes = &rest[..colon];
    if !is_digits(len_bytes) {
        return Attempt::Skip(1);
    }
    let Some(len) = parse_uint(len_bytes) else {
        return Attempt::Skip(1);
    };
    let data_start = 2 + colon + 1;
    let total = data_start + len as usize;
    if buf.len() < total {
        return Attempt::Incomplete;
    }
    Attempt::Frame(total, Frame::Data(buf[data_start..total].to_vec()))
}

/// Parses `1:{cols}:{rows}:` at the front of `buf`.
fn parse_resize(buf: &[u8]) -> Attempt {
    if buf.len() < 2 {
        return Attempt::Incomplete;
    }
    if buf[1] != b':' {
        return Attempt::Skip(1);
    }
    let rest = &buf[2..];
    let Some(c1) = rest.iter().position(|&b| b == b':') else {
        return if rest.iter().all(u8::is_ascii_digit) {
            Attempt::Incomplete
        } else {
            Attempt::Skip(1)
        };
    };
    let cols_bytes = &rest[..c1];
    if !is_digits(cols_bytes) {
        return Attempt::Skip(1);
    }
    let after_cols = &rest[c1 + 1..];
    let Some(c2) = after_cols.iter().position(|&b| b == b':') else {
        return if after_cols.iter().all(u8::is_ascii_digit) {
            Attempt::Incomplete
        } else {
            Attempt::Skip(1)
        };
    };
    let rows_bytes = &after_cols[..c2];
    if !is_digits(rows_bytes) {
        return Attempt::Skip(1);
    }
    let (Some(cols), Some(rows)) = (parse_uint(cols_bytes), parse_uint(rows_bytes)) else {
        return Attempt::Skip(1);
    };
    let consumed = 2 + c1 + 1 + c2 + 1;
    Attempt::Frame(consumed, Frame::Resize { cols, rows })
}

fn parse_one(buf: &[u8]) -> Attempt {
    match buf.first() {
        None => Attempt::Incomplete,
        Some(b'2') => Attempt::Frame(1, Frame::Keepalive),
        Some(b'0') => parse_data(buf),
        Some(b'1') => parse_resize(buf),
        Some(_) => Attempt::Skip(1),
    }
}

/// Pulls every complete frame currently in `buf`, leaving a trailing
/// partial frame (if any) in place for the next read. Never panics --
/// malformed bytes are dropped one at a time until the stream resyncs.
fn parse_frames(buf: &mut Vec<u8>) -> Vec<Frame> {
    let mut frames = Vec::new();
    loop {
        match parse_one(buf) {
            Attempt::Frame(n, frame) => {
                frames.push(frame);
                buf.drain(..n);
            }
            Attempt::Incomplete => break,
            Attempt::Skip(n) => {
                buf.drain(..n);
            }
        }
    }
    frames
}

/// Strips scheme and port off a Proxmox API host (`https://pve.example.com:8006`
/// -> `pve.example.com`) to get the SSH target. proxmox-desktop stores one
/// host per connection, so this assumes the SSH endpoint lives on the same
/// machine as the API -- true for the common single-node case; a genuine
/// multi-node cluster with per-node SSH endpoints isn't modeled yet.
fn ssh_host(api_host: &str) -> String {
    let without_scheme = api_host.rsplit("://").next().unwrap_or(api_host);
    without_scheme
        .split(':')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Opens a root shell on a connection's host over SSH and bridges it
/// through a local pve-xtermjs-speaking websocket, mirroring
/// `console.rs::open_console`'s one-shot local listener.
#[tauri::command]
pub async fn open_ssh_shell(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SshSessions>,
    connection_id: String,
) -> Result<ConsoleInfo, String> {
    let (info, ssh_info) = connections::info_and_ssh(&app, &connection_id)?;
    let host = ssh_host(&info.host);

    let secret = if ssh_info.use_agent {
        String::new()
    } else {
        connections::get_ssh_secret(&app, &connection_id)?
    };
    let auth = if ssh_info.use_agent {
        Auth::Agent
    } else if let Some(path) = &ssh_info.key_path {
        Auth::Key {
            path: Path::new(path),
            passphrase: &secret,
        }
    } else {
        Auth::Password(&secret)
    };

    let known_hosts_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;

    let handle = ssh::connect(
        &host,
        ssh_info.port,
        &ssh_info.user,
        &auth,
        &known_hosts_dir,
    )
    .await
    .map_err(|e| e.to_string())?;
    let handle = Arc::new(tokio::sync::Mutex::new(handle));

    let mut channel = handle
        .lock()
        .await
        .channel_open_session()
        .await
        .map_err(|e| ssh::describe_shell_error(&e, &host, ssh_info.port))?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            INITIAL_COLS.into(),
            INITIAL_ROWS.into(),
            0,
            0,
            &[],
        )
        .await
        .map_err(|e| ssh::describe_shell_error(&e, &host, ssh_info.port))?;
    channel
        .request_shell(false)
        .await
        .map_err(|e| ssh::describe_shell_error(&e, &host, ssh_info.port))?;

    sessions
        .0
        .lock()
        .unwrap()
        .insert(connection_id, Session { handle });

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let user = ssh_info.user.clone();
    let ticket = bridge_ticket()?;
    let expected_auth = format!("{user}:{ticket}\n");

    tauri::async_runtime::spawn(async move {
        // One shot: serve the first local connection, then the bridge dies.
        // Bounded, because until someone connects there is an authenticated
        // shell sitting idle behind this port.
        let Ok(Ok((stream, _))) = tokio::time::timeout(ACCEPT_TIMEOUT, listener.accept()).await
        else {
            return;
        };
        drop(listener);
        let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        let (mut ws_tx, mut ws_rx) = ws.split();
        let mut buf: Vec<u8> = Vec::new();
        let mut past_auth_line = false;

        'pump: loop {
            tokio::select! {
                msg = ws_rx.next() => {
                    let Some(Ok(msg)) = msg else { break 'pump; };
                    let data = match msg {
                        Message::Text(t) => t.as_bytes().to_vec(),
                        Message::Binary(b) => b.to_vec(),
                        Message::Close(_) => break 'pump,
                        _ => continue,
                    };
                    if !past_auth_line {
                        // The "{user}:{ticket}\n" auth line. For the PVE
                        // console this is checked by the *remote* endpoint, so
                        // console.rs can pass it through blindly. Nothing
                        // remote checks anything here -- the SSH session is
                        // already authenticated from the keyring, so this
                        // socket is the only gate on a root shell. Reject a
                        // client that can't produce the ticket.
                        if data != expected_auth.as_bytes() {
                            break 'pump;
                        }
                        past_auth_line = true;
                        continue;
                    }
                    buf.extend_from_slice(&data);
                    for frame in parse_frames(&mut buf) {
                        match frame {
                            Frame::Data(d) => {
                                if channel.data(&d[..]).await.is_err() { break 'pump; }
                            }
                            Frame::Resize { cols, rows } => {
                                let _ = channel.window_change(cols, rows, 0, 0).await;
                            }
                            Frame::Keepalive => {}
                        }
                    }
                }
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            if ws_tx.send(Message::binary(data.to_vec())).await.is_err() { break 'pump; }
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::ExitStatus { .. }) | None => break 'pump,
                        _ => {}
                    }
                }
            }
        }
        let _ = ws_tx.send(Message::Close(None)).await;
    });

    Ok(ConsoleInfo {
        port,
        ticket,
        user: Some(user),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_data_frame() {
        let mut buf = b"0:5:hello".to_vec();
        let frames = parse_frames(&mut buf);
        assert_eq!(frames, vec![Frame::Data(b"hello".to_vec())]);
        assert!(buf.is_empty());
    }

    #[test]
    fn parses_a_resize_frame() {
        let mut buf = b"1:80:24:".to_vec();
        let frames = parse_frames(&mut buf);
        assert_eq!(frames, vec![Frame::Resize { cols: 80, rows: 24 }]);
        assert!(buf.is_empty());
    }

    #[test]
    fn parses_a_keepalive() {
        let mut buf = b"2".to_vec();
        let frames = parse_frames(&mut buf);
        assert_eq!(frames, vec![Frame::Keepalive]);
        assert!(buf.is_empty());
    }

    #[test]
    fn parses_several_frames_back_to_back() {
        let mut buf = b"20:5:hello1:80:24:".to_vec();
        let frames = parse_frames(&mut buf);
        assert_eq!(
            frames,
            vec![
                Frame::Keepalive,
                Frame::Data(b"hello".to_vec()),
                Frame::Resize { cols: 80, rows: 24 },
            ]
        );
    }

    #[test]
    fn data_length_counts_bytes_not_a_decoded_char_count() {
        // "café" is 4 chars but 5 UTF-8 bytes -- the length prefix must be
        // treated as a raw byte count, and the payload sliced verbatim.
        let payload = "café".as_bytes();
        assert_eq!(payload.len(), 5);
        let mut buf = format!("0:{}:", payload.len()).into_bytes();
        buf.extend_from_slice(payload);
        let frames = parse_frames(&mut buf);
        assert_eq!(frames, vec![Frame::Data(payload.to_vec())]);
    }

    #[test]
    fn a_frame_split_across_two_reads_is_not_lost_and_does_not_panic() {
        let mut buf = b"0:5:hel".to_vec();
        assert!(parse_frames(&mut buf).is_empty());
        assert_eq!(buf, b"0:5:hel");
        buf.extend_from_slice(b"lo");
        assert_eq!(parse_frames(&mut buf), vec![Frame::Data(b"hello".to_vec())]);
    }

    #[test]
    fn incomplete_length_prefix_waits_instead_of_panicking() {
        let mut buf = b"0:1".to_vec();
        assert!(parse_frames(&mut buf).is_empty());
        assert_eq!(buf, b"0:1");
    }

    #[test]
    fn garbage_bytes_do_not_panic_and_get_dropped() {
        let mut buf = b"\xffnot a frame at all\x00".to_vec();
        let frames = parse_frames(&mut buf);
        assert!(frames.is_empty());
        assert!(buf.is_empty());
    }

    #[test]
    fn malformed_length_recovers_and_still_parses_what_follows() {
        // "0:xx:" has a non-numeric length -- must be skipped without
        // panicking, and a valid frame right after it must still parse.
        let mut buf = b"0:xx:2".to_vec();
        let frames = parse_frames(&mut buf);
        // Bytes are dropped one at a time until "2" (keepalive) is reached.
        assert!(frames.contains(&Frame::Keepalive));
    }

    #[test]
    fn empty_buffer_does_not_panic() {
        let mut buf: Vec<u8> = Vec::new();
        assert!(parse_frames(&mut buf).is_empty());
    }

    #[test]
    fn ssh_host_strips_scheme_and_port() {
        assert_eq!(ssh_host("https://pve.example.com:8006"), "pve.example.com");
        assert_eq!(ssh_host("http://10.0.0.5:8006"), "10.0.0.5");
        assert_eq!(ssh_host("pve.example.com"), "pve.example.com");
    }

    /// The ticket is the only thing standing between a local process and a
    /// root shell on the node. An empty or predictable one would make the
    /// auth-line gate in `open_ssh_shell` accept a bare `{user}:` line from
    /// anybody, so assert it is actually random and actually long.
    #[test]
    fn bridge_ticket_is_long_random_hex() {
        let a = bridge_ticket().expect("rng available");
        let b = bridge_ticket().expect("rng available");
        assert_eq!(a.len(), 64, "expected 32 bytes as hex");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "not hex: {a}");
        assert_ne!(a, b, "ticket must not repeat");
    }
}
