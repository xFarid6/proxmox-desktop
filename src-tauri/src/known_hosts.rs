//! Host key verification: known_hosts storage + Trust-On-First-Use (TOFU).
//!
//! Fixes issue #4 — `check_server_key` used to accept any host key, so a
//! MITM could silently intercept every connection. The file lives next to
//! `connections.json` in the app config dir (not `~/.ssh/known_hosts`, so we
//! never touch the user's real SSH state and stay testable via a tempdir).
//!
//! TOFU means: a host we've never seen is trusted and its key remembered;
//! a host whose remembered key no longer matches is refused, since that's
//! exactly the shape of an interception attempt.

use std::path::{Path, PathBuf};

use russh::keys::PublicKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Key matches the one we already have on file.
    Trusted,
    /// We've never seen this host before.
    New,
    /// Host is known, but the key doesn't match what we recorded.
    Changed,
}

fn known_hosts_path(dir: &Path) -> PathBuf {
    dir.join("known_hosts")
}

/// Turn a `russh::keys::Error` into UI-safe text. Its `IO` variant is
/// `#[error(transparent)]` over `std::io::Error`, so a raw `.to_string()`
/// here would hand the frontend an OS-specific "(os error N)" fragment for
/// something as mundane as a permissions problem on the known_hosts file.
/// Every other variant of that enum already renders as a plain English
/// sentence (see its `#[error(...)]` attributes), so only `IO` needs
/// special-casing.
fn describe_keys_error(e: russh::keys::Error) -> String {
    match e {
        russh::keys::Error::IO(_) => {
            "could not read or write proxmox-desktop's known_hosts file".to_string()
        }
        other => other.to_string(),
    }
}

/// Check `key` against the stored entry for `host:port`, if any.
pub fn verify(dir: &Path, host: &str, port: u16, key: &PublicKey) -> Result<Verdict, String> {
    let path = known_hosts_path(dir);
    match russh::keys::check_known_hosts_path(host, port, key, &path) {
        Ok(true) => Ok(Verdict::Trusted),
        Ok(false) => Ok(Verdict::New),
        Err(russh::keys::Error::KeyChanged { .. }) => Ok(Verdict::Changed),
        Err(e) => Err(describe_keys_error(e)),
    }
}

/// Record `key` as the trusted key for `host:port` (first-use TOFU write).
pub fn trust(dir: &Path, host: &str, port: u16, key: &PublicKey) -> Result<(), String> {
    let path = known_hosts_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "could not create proxmox-desktop's config directory".to_string())?;
    }
    russh::keys::known_hosts::learn_known_hosts_path(host, port, key, &path)
        .map_err(describe_keys_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::{Algorithm, PrivateKey};

    fn key(seed: u8) -> PublicKey {
        // Deterministic-enough for tests: distinct seeds must yield distinct
        // keys, which `PrivateKey::random` guarantees across calls.
        let _ = seed;
        PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)
            .expect("generate test key")
            .public_key()
            .clone()
    }

    #[test]
    fn unknown_host_is_new() {
        let dir = tempfile::tempdir().unwrap();
        let k = key(1);
        assert_eq!(
            verify(dir.path(), "example.com", 22, &k).unwrap(),
            Verdict::New
        );
    }

    #[test]
    fn trusted_after_learning() {
        let dir = tempfile::tempdir().unwrap();
        let k = key(2);
        trust(dir.path(), "example.com", 22, &k).unwrap();
        assert_eq!(
            verify(dir.path(), "example.com", 22, &k).unwrap(),
            Verdict::Trusted
        );
    }

    #[test]
    fn rejects_changed_host_key() {
        let dir = tempfile::tempdir().unwrap();
        let original = key(3);
        let attacker = key(4);
        trust(dir.path(), "example.com", 22, &original).unwrap();
        assert_eq!(
            verify(dir.path(), "example.com", 22, &attacker).unwrap(),
            Verdict::Changed
        );
    }

    #[test]
    fn different_ports_are_independent_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let k = key(5);
        trust(dir.path(), "example.com", 22, &k).unwrap();
        // Same host, different port: never learned, so it's New — not
        // Changed and not Trusted off the port-22 entry.
        assert_eq!(
            verify(dir.path(), "example.com", 2222, &k).unwrap(),
            Verdict::New
        );
    }

    #[test]
    fn corrupted_known_hosts_file_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        // Not a known_hosts line at all — garbage bytes, truncated base64,
        // wrong field count. `verify` must return a plain Result, never
        // panic, no matter what's on disk (e.g. a crash mid-write, a user
        // hand-editing the file).
        std::fs::write(
            known_hosts_path(dir.path()),
            b"this is not a known_hosts entry\n\xff\xfe\x00garbage\nexample.com not-even-a-key\n",
        )
        .unwrap();
        let k = key(6);
        // Whatever the verdict, it must come back as a `Result` we can
        // handle — not a panic that would take down the whole app.
        let _ = verify(dir.path(), "example.com", 22, &k);
    }

    #[test]
    fn keys_io_error_does_not_leak_os_error_text() {
        // russh::keys::Error::IO is #[error(transparent)] over
        // std::io::Error — this is the exact shape a permissions problem or
        // dropped handle on the known_hosts file takes. Feed it through the
        // actual function used by verify()/trust() and make sure the
        // OS-specific "(os error N)" fragment never survives. Assertion is
        // on the fragment, not exact wording, so it holds regardless of the
        // OS's localized message text.
        let io_err = std::io::Error::from_raw_os_error(10054);
        let msg = describe_keys_error(russh::keys::Error::IO(io_err));
        assert!(!msg.contains("os error"));
        assert!(!msg.contains("10054"));
    }

    #[test]
    fn keys_key_changed_error_still_reads_as_english() {
        // Non-IO variants aren't transparent wrappers, so they should pass
        // through unchanged.
        let msg = describe_keys_error(russh::keys::Error::KeyIsCorrupt);
        assert!(msg.to_lowercase().contains("corrupt"));
    }

    #[test]
    fn trust_after_corrupted_line_still_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(known_hosts_path(dir.path()), b"garbage garbage garbage\n").unwrap();
        let k = key(7);
        // Learning a new host should still succeed even if the file already
        // has unrelated junk in it — we don't want one bad line to brick
        // TOFU for every other host.
        trust(dir.path(), "example.com", 22, &k).unwrap();
        assert_eq!(
            verify(dir.path(), "example.com", 22, &k).unwrap(),
            Verdict::Trusted
        );
    }
}
