#[cfg(target_os = "android")]
mod android_keystore;
pub mod commands;
pub mod connections;
pub mod console;
pub mod docker;
pub mod host;
pub mod known_hosts;
pub mod llm;
pub mod proxmox;
pub mod scan;
pub mod ssh;
pub mod ssh_console;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_keystore::init());
    builder
        .manage(ssh::SshSessions::default())
        .manage(llm::LlmCancels::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::scan_lan,
            commands::scan_tailscale,
            commands::cluster_resources,
            commands::guest_power,
            commands::node_tasks,
            commands::apt_update,
            commands::task_status,
            commands::task_log,
            commands::node_network,
            commands::create_network_iface,
            commands::update_network_iface,
            commands::delete_network_iface,
            commands::apply_network,
            commands::revert_network,
            commands::guest_config,
            commands::set_guest_config,
            commands::resize_disk,
            commands::node_storages,
            commands::storage_content,
            commands::create_guest,
            commands::vzdump,
            commands::delete_volume,
            commands::backup_jobs,
            commands::replication_jobs,
            commands::firewall_rules,
            commands::add_firewall_rule,
            commands::delete_firewall_rule,
            commands::firewall_options,
            commands::set_firewall_options,
            commands::storage_configs,
            commands::add_storage,
            commands::delete_storage,
            commands::access_users,
            commands::add_user,
            commands::delete_user,
            commands::access_domains,
            commands::access_roles,
            commands::access_acl,
            commands::access_permissions,
            commands::set_acl,
            commands::ha_resources,
            commands::add_ha_resource,
            commands::update_ha_resource,
            commands::delete_ha_resource,
            commands::ha_groups,
            commands::add_ha_group,
            commands::update_ha_group,
            commands::delete_ha_group,
            commands::ha_status_current,
            commands::ceph_status,
            commands::ceph_osds,
            commands::ceph_pools,
            commands::ceph_services,
            commands::ceph_osd_in_out,
            commands::ceph_osd_power,
            commands::ceph_osd_destroy,
            commands::ceph_pool_create,
            commands::ceph_pool_update,
            commands::ceph_pool_delete,
            commands::certificates_info,
            commands::upload_certificate,
            commands::delete_custom_certificate,
            commands::acme_order_certificate,
            commands::acme_renew_certificate,
            commands::acme_accounts,
            commands::acme_account,
            commands::acme_plugins,
            console::open_console,
            ssh_console::open_ssh_shell,
            docker::docker_ps,
            docker::docker_action,
            docker::docker_logs,
            docker::host_docker_ps,
            host::host_ports,
            host::host_services,
            host::host_streams,
            llm::llm_probe,
            llm::llm_set_endpoint,
            llm::llm_chat,
            llm::llm_cancel,
            llm::llm_models_available,
            llm::llm_switch_model,
            llm::llm_health,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
