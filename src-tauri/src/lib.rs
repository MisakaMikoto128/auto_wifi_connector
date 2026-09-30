pub mod monitor;
pub mod netcheck;
pub mod wifi;

use monitor::{MonitorSnapshot, Shared};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

/// 扫描可见 WiFi 网络（标注已保存配置与当前连接）。
#[tauri::command]
fn get_wifi_list() -> Result<Vec<wifi::WifiNetwork>, String> {
    wifi::scan()
}

/// 当前监控状态快照。
#[tauri::command]
fn get_snapshot(state: State<'_, Shared>) -> MonitorSnapshot {
    state.lock().unwrap().clone()
}

/// 启用或停用自动断网重连轮询。
#[tauri::command]
fn set_auto_reconnect(app: AppHandle, state: State<'_, Shared>, enabled: bool) -> MonitorSnapshot {
    let snapshot = {
        let mut guard = state.lock().unwrap();
        guard.enabled = enabled;
        guard.clone()
    };
    let _ = app.emit("monitor", &snapshot);
    snapshot
}

/// 开机自启当前状态。
#[tauri::command]
fn autostart_is_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 设置开机自启，返回设置后的实际状态。
#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> bool {
    let launcher = app.autolaunch();
    let _ = if enabled { launcher.enable() } else { launcher.disable() };
    launcher.is_enabled().unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(monitor::new_shared())
        .invoke_handler(tauri::generate_handler![
            get_wifi_list,
            get_snapshot,
            set_auto_reconnect,
            autostart_is_enabled,
            autostart_set,
        ])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .menu(&menu)
                .tooltip("Auto WiFi Connector")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // 关闭窗口时隐藏到托盘，而非退出
            let window = app.get_webview_window("main").unwrap();
            let handle = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = handle.hide();
                    api.prevent_close();
                }
            });

            // 默认注册开机自启，可在界面中关闭
            let _ = app.autolaunch().enable();

            monitor::spawn(app.handle().clone(), app.state::<Shared>().inner().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
