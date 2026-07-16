//! Android token store: routes to the Kotlin `KeystorePlugin`
//! (EncryptedSharedPreferences, AES-256 master key in the Android Keystore).
//! Desktop targets use the OS keyring in `connections` instead.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    AppHandle, Manager, Runtime,
};

struct Keystore<R: Runtime>(PluginHandle<R>);

#[derive(Serialize)]
struct TokenArgs<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<&'a str>,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct Empty {}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("keystore")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin("com.xfarid.proxmox_desktop", "KeystorePlugin")?;
            app.manage(Keystore(handle));
            Ok(())
        })
        .build()
}

pub fn get(app: &AppHandle, id: &str) -> Result<String, String> {
    app.state::<Keystore<tauri::Wry>>()
        .0
        .run_mobile_plugin::<TokenResponse>("getToken", TokenArgs { id, token: None })
        .map(|r| r.token)
        .map_err(|e| e.to_string())
}

pub fn set(app: &AppHandle, id: &str, token: &str) -> Result<(), String> {
    app.state::<Keystore<tauri::Wry>>()
        .0
        .run_mobile_plugin::<Empty>(
            "setToken",
            TokenArgs {
                id,
                token: Some(token),
            },
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    app.state::<Keystore<tauri::Wry>>()
        .0
        .run_mobile_plugin::<Empty>("deleteToken", TokenArgs { id, token: None })
        .map(|_| ())
        .map_err(|e| e.to_string())
}
