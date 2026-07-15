//! Local websocket bridge for the embedded console.
//!
//! The webview's WebSocket API cannot send an Authorization header, so the
//! backend dials the authenticated `wss://` Proxmox endpoint itself and pipes
//! bytes to a one-shot listener on 127.0.0.1. The API token stays in Rust;
//! the frontend only ever sees the short-lived one-time console ticket, which
//! the VNC/term protocols themselves require as a password.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;

use crate::connections;
use crate::proxmox::types::GuestKind;
use crate::proxmox::Client;

/// What the frontend needs to attach a console widget.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleInfo {
    /// Local bridge port — connect to `ws://127.0.0.1:{port}`.
    pub port: u16,
    /// One-time console ticket: VNC password (vnc) or auth line (term).
    pub ticket: String,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleMode {
    Vnc,
    Term,
}

/// Percent-encode a query value (tickets contain `:` `+` `/` `=`).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// rustls verifier that accepts any server cert — used only when the
/// connection was saved with the explicit self-signed opt-in.
#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn tls_connector(accept_invalid_certs: bool) -> Result<Connector, String> {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;
    let config = if accept_invalid_certs {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider)))
            .with_no_client_auth()
    } else {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        builder.with_root_certificates(roots).with_no_client_auth()
    };
    Ok(Connector::Rustls(Arc::new(config)))
}

/// Open a console websocket bridge. Fetches a vncproxy/termproxy ticket,
/// binds a one-shot local listener, and pipes it to the remote endpoint.
#[tauri::command]
pub async fn open_console(
    app: tauri::AppHandle,
    connection_id: String,
    node: String,
    kind: GuestKind,
    vmid: u32,
    mode: ConsoleMode,
) -> Result<ConsoleInfo, String> {
    let (info, token) = connections::info_and_token(&app, &connection_id)?;
    let client =
        Client::new(&info.host, &token, info.accept_invalid_certs).map_err(|e| e.to_string())?;

    let (ticket, remote_port, user) = match mode {
        ConsoleMode::Vnc => {
            let p = client
                .vncproxy(&node, kind, vmid)
                .await
                .map_err(|e| e.to_string())?;
            (p.ticket, p.port.to_string(), p.user)
        }
        ConsoleMode::Term => {
            let p = client
                .termproxy(&node, kind, vmid)
                .await
                .map_err(|e| e.to_string())?;
            (p.ticket, p.port.to_string(), p.user)
        }
    };

    let ws_host = info
        .host
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let url = format!(
        "{ws_host}/api2/json/nodes/{node}/{}/{vmid}/vncwebsocket?port={remote_port}&vncticket={}",
        kind.as_path(),
        urlencode(&ticket),
    );

    let mut request = url.into_client_request().map_err(|e| e.to_string())?;
    request.headers_mut().insert(
        "Authorization",
        format!("PVEAPIToken={token}")
            .parse()
            .map_err(|_| "bad auth header".to_string())?,
    );
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());

    let connector = tls_connector(info.accept_invalid_certs)?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    tauri::async_runtime::spawn(async move {
        // One shot: serve the first local connection, then the bridge dies.
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        drop(listener);
        let Ok(local) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        let Ok((remote, _)) =
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await
        else {
            return;
        };

        let (mut local_tx, mut local_rx) = local.split();
        let (mut remote_tx, mut remote_rx) = remote.split();

        let to_remote = async {
            while let Some(Ok(msg)) = local_rx.next().await {
                if matches!(msg, Message::Close(_)) || remote_tx.send(msg).await.is_err() {
                    break;
                }
            }
            let _ = remote_tx.send(Message::Close(None)).await;
        };
        let to_local = async {
            while let Some(Ok(msg)) = remote_rx.next().await {
                if matches!(msg, Message::Close(_)) || local_tx.send(msg).await.is_err() {
                    break;
                }
            }
            let _ = local_tx.send(Message::Close(None)).await;
        };
        futures_util::join!(to_remote, to_local);
    });

    Ok(ConsoleInfo { port, ticket, user })
}
