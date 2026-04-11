use dev_command_center_tauri::daemon_runtime::DaemonService;
use std::path::PathBuf;

fn main() {
    let db_path = std::env::var_os("DCC_DAEMON_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("database.sqlite"));
    let app_data_dir = std::env::var_os("DCC_DAEMON_APP_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let runtime_path = dev_command_center_tauri::daemon_client::runtime_file(&app_data_dir);

    let service = match DaemonService::new(db_path) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("[DCC][dccd] failed to initialize daemon: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = dev_command_center_tauri::daemon_runtime::serve(service, &runtime_path) {
        eprintln!("[DCC][dccd] {error}");
        std::process::exit(1);
    }
}
