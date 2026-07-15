pub mod commands;
pub mod connections;
pub mod proxmox;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::cluster_resources,
            commands::guest_power,
            commands::node_tasks,
            commands::task_status,
            commands::task_log,
            commands::node_network,
            commands::guest_config,
            commands::set_guest_config,
            commands::resize_disk,
            commands::node_storages,
            commands::storage_content,
            commands::create_guest,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
