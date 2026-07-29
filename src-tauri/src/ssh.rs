//! SSH connect + auth + host-key verification, ported from hopline's
//! `ssh.rs` (its issue #23 port). Trimmed to what this app needs: one PTY
//! shell channel over an authenticated connection. No SFTP, port
//! forwarding, jump hosts, or keygen -- those stay in hopline.
//!
//! Unlike hopline, nothing here pumps bytes to the frontend via Tauri
//! events: `ssh_console.rs` bridges the shell channel to a local websocket
//! instead, so the existing pve-xtermjs terminal (`ConsoleView.vue`) works
//! unchanged. This module only gets the connection to the point of an open
//! shell channel.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::PrivateKeyWithHashAlg;

use crate::known_hosts::{self, Verdict};

/// How long we'll wait for the TCP connect + SSH handshake before giving up
/// and reporting a timeout. Auth (which can involve a human typing a
/// passphrase) is not covered by this -- only reaching the host.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// One open connection: the russh `Handle`, kept alive behind a
/// `tokio::sync::Mutex` after the shell channel opens so a follow-up
/// feature (issue #65) can open a second channel on the same authenticated
/// connection instead of reconnecting.
#[derive(Clone)]
pub struct Session {
    pub handle: Arc<tokio::sync::Mutex<Handle<ClientHandler>>>,
}

/// Open SSH sessions, keyed by connection id. One shell per connection at a
/// time: opening a new one replaces the old entry (the old channel's own
/// bridge task keeps that connection alive until it closes on its own --
/// see `ssh_console.rs`).
#[derive(Default)]
pub struct SshSessions(pub Mutex<HashMap<String, Session>>);

pub struct ClientHandler {
    host: String,
    port: u16,
    known_hosts_dir: PathBuf,
}

/// Rich enough to build a UI-facing message in `ConnectError::from_handler_error`,
/// but still convertible from a plain `russh::Error` so `?` keeps working
/// inside russh.
#[derive(Debug)]
pub enum HandlerError {
    Ssh(russh::Error),
    HostKeyChanged { host: String, port: u16 },
    KnownHosts(String),
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandlerError::Ssh(e) => write!(f, "{e}"),
            HandlerError::HostKeyChanged { host, port } => write!(
                f,
                "host key for {host}:{port} does not match the key we trusted before. \
                 This could mean the server was reinstalled, or that someone is \
                 intercepting the connection — refusing to connect. If you're sure \
                 this is expected, remove the old entry from proxmox-desktop's \
                 known_hosts file.",
            ),
            HandlerError::KnownHosts(msg) => write!(f, "known_hosts check failed: {msg}"),
        }
    }
}

impl std::error::Error for HandlerError {}

impl From<russh::Error> for HandlerError {
    fn from(e: russh::Error) -> Self {
        HandlerError::Ssh(e)
    }
}

/// The UI-facing shape of a failed SSH connect/shell attempt. Every
/// failure is funneled through this -- never a bare `russh::Error`/`io::Error`
/// `to_string()` -- so the frontend gets a message a user can act on, plus
/// a `kind` a caller can use to decide what "retry" should mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectError {
    pub kind: ConnectErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectErrorKind {
    /// Reaching the host took too long -- dead IP, firewall dropping
    /// packets, wrong network.
    Timeout,
    /// The host actively refused the connection or DNS/routing failed.
    Unreachable,
    /// TOFU: the key on file for this host:port no longer matches what the
    /// server presented.
    HostKeyChanged,
    /// The server rejected the credentials.
    AuthFailed,
    /// Everything else (protocol errors, unreadable key file, ...) --
    /// still a readable sentence, just not one of the common cases above.
    Other,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConnectError {}

impl ConnectError {
    fn timeout(host: &str, port: u16) -> Self {
        ConnectError {
            kind: ConnectErrorKind::Timeout,
            message: format!(
                "Connecting to {host}:{port} timed out after {}s. Check the \
                 address, or whether a firewall is silently dropping the connection.",
                CONNECT_TIMEOUT.as_secs()
            ),
        }
    }

    fn auth_failed(user: &str) -> Self {
        ConnectError {
            kind: ConnectErrorKind::AuthFailed,
            message: format!(
                "Could not sign in as \"{user}\" — check the password, \
                 passphrase, or key file and try again."
            ),
        }
    }

    fn from_key_load_error(e: &russh::keys::Error) -> Self {
        use russh::keys::Error as KeyError;
        let detail = match e {
            KeyError::KeyIsEncrypted => {
                "the key file is encrypted and needs a passphrase".to_string()
            }
            KeyError::KeyIsCorrupt => "the key file is corrupt or isn't a private key".to_string(),
            KeyError::IO(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                "the key file couldn't be found at that path".to_string()
            }
            _ => "the key file couldn't be read".to_string(),
        };
        ConnectError {
            kind: ConnectErrorKind::Other,
            message: format!("Could not load the private key: {detail}."),
        }
    }

    /// Classify a failure from `client::connect` (TCP + handshake + host
    /// key check). `russh::Error::IO` and the nested
    /// `russh::Error::Keys(keys::Error::IO(_))` are both
    /// `#[error(transparent)]` over `std::io::Error`, so without these two
    /// arms a connection drop during the handshake would leak a raw
    /// "(os error N)" string through the `HandlerError::Ssh(other)` catch-all.
    fn from_handler_error(e: HandlerError, host: &str, port: u16) -> Self {
        match e {
            HandlerError::HostKeyChanged { host, port } => ConnectError {
                kind: ConnectErrorKind::HostKeyChanged,
                message: format!(
                    "The identity {host}:{port} presented doesn't match the one \
                     proxmox-desktop remembered from last time. This happens after a \
                     server reinstall — it's also exactly what a \
                     machine-in-the-middle attack looks like, so proxmox-desktop is \
                     refusing to connect. If you're sure this is expected, \
                     remove the old entry from proxmox-desktop's known_hosts file \
                     and try again."
                ),
            },
            HandlerError::Ssh(russh::Error::IO(io_err)) => Self::from_io_error(&io_err, host, port),
            HandlerError::Ssh(russh::Error::Keys(russh::keys::Error::IO(io_err))) => {
                Self::from_io_error(&io_err, host, port)
            }
            HandlerError::Ssh(other) => ConnectError {
                kind: ConnectErrorKind::Other,
                message: other.to_string(),
            },
            HandlerError::KnownHosts(msg) => ConnectError {
                kind: ConnectErrorKind::Other,
                message: format!("Could not verify the host's identity: {msg}"),
            },
        }
    }

    /// Classify a failure from the auth-phase calls (`best_supported_rsa_hash`,
    /// `authenticate_publickey`, `authenticate_password`). Same transparent-IO
    /// unwrapping as `from_handler_error` -- a connection reset mid-auth would
    /// otherwise leak a raw "(os error N)" string. A handful of variants
    /// unambiguously mean the server ended or rejected authentication, so
    /// those map to `AuthFailed`; everything else gets a clean generic
    /// sentence instead of the wire-level `Display` text.
    fn from_auth_error(e: &russh::Error, host: &str, port: u16, user: &str) -> Self {
        use russh::keys::Error as KeyError;
        use russh::Error as SshError;
        match e {
            SshError::IO(io_err) => Self::from_io_error(io_err, host, port),
            SshError::Keys(KeyError::IO(io_err)) => Self::from_io_error(io_err, host, port),
            SshError::NotAuthenticated
            | SshError::NoAuthMethod
            | SshError::Disconnect
            | SshError::HUP => Self::auth_failed(user),
            _ => ConnectError {
                kind: ConnectErrorKind::Other,
                message: format!(
                    "Something went wrong while signing in to {host}:{port}. \
                     Check the connection details and try again."
                ),
            },
        }
    }

    fn from_io_error(io_err: &std::io::Error, host: &str, port: u16) -> Self {
        use std::io::ErrorKind;
        match io_err.kind() {
            ErrorKind::TimedOut => Self::timeout(host, port),
            ErrorKind::ConnectionRefused => ConnectError {
                kind: ConnectErrorKind::Unreachable,
                message: format!(
                    "{host}:{port} refused the connection — is the SSH \
                     server running there, and is the port correct?"
                ),
            },
            _ => ConnectError {
                kind: ConnectErrorKind::Unreachable,
                message: format!(
                    "Could not reach {host}:{port}. Check the address and \
                     that the host is up and reachable on the network."
                ),
            },
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = HandlerError;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        match known_hosts::verify(&self.known_hosts_dir, &self.host, self.port, key)
            .map_err(HandlerError::KnownHosts)?
        {
            Verdict::Trusted => Ok(true),
            Verdict::New => {
                known_hosts::trust(&self.known_hosts_dir, &self.host, self.port, key)
                    .map_err(HandlerError::KnownHosts)?;
                Ok(true)
            }
            Verdict::Changed => Err(HandlerError::HostKeyChanged {
                host: self.host.clone(),
                port: self.port,
            }),
        }
    }
}

type DynAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Connects to the platform's running agent: ssh-agent via `SSH_AUTH_SOCK`
/// on Unix, the Windows OpenSSH agent's named pipe on Windows (falling back
/// to Pageant if that's not running). `.dynamic()` erases the
/// platform-specific stream type so the caller doesn't need `cfg`.
#[cfg(unix)]
async fn connect_agent() -> Result<DynAgentClient, ConnectError> {
    AgentClient::connect_env()
        .await
        .map(AgentClient::dynamic)
        .map_err(|_| ConnectError {
            kind: ConnectErrorKind::Other,
            message: "Could not reach an SSH agent. Check that one is running and \
                     SSH_AUTH_SOCK is set."
                .to_string(),
        })
}

#[cfg(windows)]
async fn connect_agent() -> Result<DynAgentClient, ConnectError> {
    if let Ok(client) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        return Ok(client.dynamic());
    }
    Ok(AgentClient::connect_pageant().await.dynamic())
}

/// Authenticates using whichever of the agent's loaded identities the
/// server accepts first. A rejected key just moves on to the next one; a
/// failure signing with the agent itself aborts the loop instead of
/// silently trying the rest.
///
/// Explicitly boxed: `authenticate_publickey_with`'s `S: auth::Signer` bound
/// (RPITIT under the hood) otherwise produces a "Send is not general
/// enough" error from tauri::command's macro-generated Future once this is
/// called from `connect()` -- a known rustc/async-trait HRTB inference
/// limitation, not a real soundness issue. Boxing gives the compiler a
/// concrete, already-erased Future type to reason about instead.
fn authenticate_via_agent<'a>(
    session: &'a mut Handle<ClientHandler>,
    user: &'a str,
    host: &'a str,
    port: u16,
) -> Pin<Box<dyn Future<Output = Result<russh::client::AuthResult, ConnectError>> + Send + 'a>> {
    Box::pin(async move {
        let mut agent = connect_agent().await?;
        let identities = agent.request_identities().await.map_err(|_| ConnectError {
            kind: ConnectErrorKind::Other,
            message: "Could not list identities from the SSH agent.".to_string(),
        })?;
        if identities.is_empty() {
            return Err(ConnectError {
                kind: ConnectErrorKind::Other,
                message: "The SSH agent has no keys loaded. Add one (ssh-add, or load it into \
                 Pageant) and try again."
                    .to_string(),
            });
        }
        let best_hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| ConnectError::from_auth_error(&e, host, port, user))?
            .flatten();
        for key in identities {
            let attempt: Pin<
                Box<
                    dyn Future<Output = Result<russh::client::AuthResult, russh::AgentAuthError>>
                        + Send
                        + '_,
                >,
            > = Box::pin(session.authenticate_publickey_with(
                user.to_string(),
                key,
                best_hash,
                &mut agent,
            ));
            match attempt.await {
                Ok(result) if result.success() => return Ok(result),
                Ok(_) => continue,
                Err(_) => {
                    return Err(ConnectError {
                        kind: ConnectErrorKind::Other,
                        message: "The SSH agent failed to sign the authentication request."
                            .to_string(),
                    })
                }
            }
        }
        Err(ConnectError::auth_failed(user))
    })
}

/// One of the three auth methods the connection form offers: an unlocked
/// or passphrase-protected key file, the platform's running ssh-agent, or a
/// plain password.
pub enum Auth<'a> {
    Key {
        path: &'a Path,
        /// Key passphrase; empty means the key isn't encrypted.
        passphrase: &'a str,
    },
    Agent,
    Password(&'a str),
}

/// Authenticates an already-transport-connected session.
async fn authenticate(
    session: &mut Handle<ClientHandler>,
    host: &str,
    port: u16,
    user: &str,
    auth: &Auth<'_>,
) -> Result<(), ConnectError> {
    let authed = match auth {
        Auth::Agent => authenticate_via_agent(session, user, host, port).await?,
        Auth::Key { path, passphrase } => {
            let passphrase = if passphrase.is_empty() {
                None
            } else {
                Some(*passphrase)
            };
            let key = russh::keys::load_secret_key(path, passphrase)
                .map_err(|e| ConnectError::from_key_load_error(&e))?;
            let best_hash = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| ConnectError::from_auth_error(&e, host, port, user))?
                .flatten();
            session
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), best_hash))
                .await
                .map_err(|e| ConnectError::from_auth_error(&e, host, port, user))?
        }
        Auth::Password(password) => session
            .authenticate_password(user, *password)
            .await
            .map_err(|e| ConnectError::from_auth_error(&e, host, port, user))?,
    };
    if !authed.success() {
        return Err(ConnectError::auth_failed(user));
    }
    Ok(())
}

/// Connects and authenticates. `known_hosts_dir` is the app config dir --
/// the TOFU file lives at `{known_hosts_dir}/known_hosts`, never at the
/// user's real `~/.ssh/known_hosts`.
pub async fn connect(
    host: &str,
    port: u16,
    user: &str,
    auth: &Auth<'_>,
    known_hosts_dir: &Path,
) -> Result<Handle<ClientHandler>, ConnectError> {
    let handler = ClientHandler {
        host: host.to_string(),
        port,
        known_hosts_dir: known_hosts_dir.to_path_buf(),
    };
    let mut session = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(Arc::new(client::Config::default()), (host, port), handler),
    )
    .await
    {
        Err(_elapsed) => return Err(ConnectError::timeout(host, port)),
        Ok(Err(e)) => return Err(ConnectError::from_handler_error(e, host, port)),
        Ok(Ok(session)) => session,
    };
    authenticate(&mut session, host, port, user, auth).await?;
    Ok(session)
}

/// What one non-interactive `exec` channel produced. `exit_status` is the
/// remote command's own exit code, so a caller can tell "ran, said no" (127
/// from a missing binary) apart from "could not run".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: u32,
}

/// Runs one command on an already-authenticated connection and collects its
/// output, the non-interactive sibling of the PTY shell `ssh_console.rs`
/// opens. No PTY is requested: this is for machine-readable output, and a
/// PTY would inject terminal control bytes into it.
///
/// Opens its own channel, so it composes with a shell channel already open
/// on the same connection.
///
/// Deliberately not bounded by a timeout here -- how long a remote command
/// may legitimately take is the caller's business, and the caller wraps this
/// in `tokio::time::timeout` with a limit that suits it.
pub async fn exec(
    handle: &Handle<ClientHandler>,
    command: &str,
) -> Result<ExecOutput, russh::Error> {
    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, command).await?;

    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let mut exit_status = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
            // ext 1 is stderr per RFC 4254; anything else is undefined by the
            // spec, so fold it into stdout rather than dropping bytes.
            russh::ChannelMsg::ExtendedData { ref data, ext } => {
                if ext == 1 {
                    stderr.extend_from_slice(data);
                } else {
                    stdout.extend_from_slice(data);
                }
            }
            russh::ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            _ => {}
        }
    }

    Ok(ExecOutput {
        // Lossy on purpose: a container name or log line with a stray byte
        // should degrade to a replacement char, never fail the whole call.
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        // No exit-status message means the channel closed without one --
        // treat it as a failure rather than a silent success.
        exit_status: exit_status.unwrap_or(1),
    })
}

/// Classify a failure from the shell-open calls (`channel_open_session`,
/// `request_pty`, `request_shell`). Same transparent-IO leak as
/// `ConnectError::from_auth_error` -- a connection drop right after auth
/// would otherwise hand the UI a raw "(os error N)" string. No `AuthFailed`
/// bucket: nothing at this phase is a rejected-credentials shape.
pub fn describe_shell_error(e: &russh::Error, host: &str, port: u16) -> String {
    match e {
        russh::Error::IO(io_err) => ConnectError::from_io_error(io_err, host, port).message,
        _ => format!(
            "Could not open a shell on {host}:{port}. The connection may \
             have been interrupted — try again."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_message_names_host_and_port() {
        let e = ConnectError::timeout("wyse-server", 22);
        assert_eq!(e.kind, ConnectErrorKind::Timeout);
        assert!(e.message.contains("wyse-server:22"));
        assert!(e.message.contains("timed out"));
    }

    #[test]
    fn auth_failed_names_the_user_and_reads_as_english() {
        let e = ConnectError::auth_failed("root");
        assert_eq!(e.kind, ConnectErrorKind::AuthFailed);
        assert!(e.message.contains("root"));
        assert!(!e.message.to_lowercase().contains("russh"));
    }

    #[test]
    fn host_key_changed_is_a_distinct_kind_with_actionable_text() {
        let handler_err = HandlerError::HostKeyChanged {
            host: "vps".into(),
            port: 2222,
        };
        let e = ConnectError::from_handler_error(handler_err, "vps", 2222);
        assert_eq!(e.kind, ConnectErrorKind::HostKeyChanged);
        assert!(e.message.contains("vps:2222"));
        assert!(e.message.to_lowercase().contains("known_hosts"));
    }

    #[test]
    fn handshake_nested_keys_io_error_does_not_leak_os_error_text() {
        let io_err = std::io::Error::from_raw_os_error(10054);
        let handler_err = HandlerError::Ssh(russh::Error::Keys(russh::keys::Error::IO(io_err)));
        let e = ConnectError::from_handler_error(handler_err, "10.0.0.9", 22);
        assert!(!e.message.contains("os error"));
        assert!(!e.message.contains("10054"));
    }

    #[test]
    fn connection_refused_is_classified_as_unreachable() {
        let io_err = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let e = ConnectError::from_io_error(&io_err, "10.0.0.9", 22);
        assert_eq!(e.kind, ConnectErrorKind::Unreachable);
        assert!(e.message.contains("10.0.0.9:22"));
    }

    #[test]
    fn io_timeout_kind_maps_to_our_timeout_kind() {
        let io_err = std::io::Error::from(std::io::ErrorKind::TimedOut);
        let e = ConnectError::from_io_error(&io_err, "10.0.0.9", 22);
        assert_eq!(e.kind, ConnectErrorKind::Timeout);
    }

    #[test]
    fn unclassified_io_error_still_reads_as_a_sentence_not_an_os_error_code() {
        let io_err = std::io::Error::other("os error 10061");
        let e = ConnectError::from_io_error(&io_err, "10.0.0.9", 22);
        assert_eq!(e.kind, ConnectErrorKind::Unreachable);
        assert!(!e.message.contains("os error"));
    }

    #[test]
    fn connect_error_display_is_just_the_message() {
        let e = ConnectError::auth_failed("plumber");
        assert_eq!(e.to_string(), e.message);
    }

    #[test]
    fn auth_phase_io_reset_does_not_leak_os_error_text() {
        let io_err = std::io::Error::from_raw_os_error(10054);
        let ssh_err = russh::Error::IO(io_err);
        let e = ConnectError::from_auth_error(&ssh_err, "10.0.0.9", 22, "root");
        assert!(!e.message.contains("os error"));
        assert!(!e.message.contains("10054"));
    }

    #[test]
    fn auth_phase_disconnect_is_classified_as_auth_failed() {
        let e = ConnectError::from_auth_error(&russh::Error::Disconnect, "vps", 22, "root");
        assert_eq!(e.kind, ConnectErrorKind::AuthFailed);
        assert!(e.message.contains("root"));
    }

    #[test]
    fn auth_phase_unclassified_variant_reads_as_a_clean_sentence() {
        let e = ConnectError::from_auth_error(&russh::Error::WrongServerSig, "vps", 22, "root");
        assert_eq!(e.kind, ConnectErrorKind::Other);
        assert!(!e.message.to_lowercase().contains("russh"));
        assert!(!e.message.contains("os error"));
    }

    #[test]
    fn shell_phase_io_drop_does_not_leak_os_error_text() {
        let io_err = std::io::Error::from_raw_os_error(10054);
        let msg = describe_shell_error(&russh::Error::IO(io_err), "10.0.0.9", 22);
        assert!(!msg.contains("os error"));
        assert!(!msg.contains("10054"));
    }

    #[test]
    fn shell_phase_unclassified_variant_reads_as_a_clean_sentence() {
        let msg = describe_shell_error(&russh::Error::WrongServerSig, "vps", 22);
        assert!(!msg.to_lowercase().contains("russh"));
        assert!(!msg.contains("os error"));
        assert!(msg.contains("vps:22"));
    }
}
