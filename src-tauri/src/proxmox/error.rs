use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Proxmox API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("unexpected response shape: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Tauri commands need serializable errors; string form is enough for the UI.
impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
