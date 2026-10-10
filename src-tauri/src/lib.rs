pub mod monitor;
pub mod netcheck;
pub mod wifi;

use monitor::{MonitorSnapshot, Shared};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

/// 扫描可见 WiFi 网络（标注已保存配置与当前连接）。
#[tauri::command]
fn get_wifi_list() -> Result<Vec<wifi::WifiNetwork>, String> {
    wifi::scan()
}

/// 手动连接指定网络（需已保存配置）。
#[tauri::command]
fn connect_wifi(ssid: String) -> Result<(), String> {
    wifi::connect(&ssid)
}

/// 当前连接的详细信息（频段、信道、速率等）；未连接时返回 None。
#[tauri::command]
fn get_connection_info() -> Option<wifi::ConnectionInfo> {
    wifi::connection_info()
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
    let _ = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    launcher.is_enabled().unwrap_or(false)
}

/// 显示并聚焦主窗口。
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 删除 exe 自身的 MOTW（Mark of the Web）网络来源标记。
///
/// 从网络下载的 exe 会被浏览器附加 Zone.Identifier 流；
/// 带该标记的 exe 被开机自启项启动时，Windows 会显示
/// 「无法验证发布者」的授权框。首次运行即删除标记，
/// 此后开机自启不再出现该提示。
fn remove_mark_of_the_web() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let ads = format!("{}:Zone.Identifier", exe.display());
    let _ = std::fs::remove_file(ads);
}

/// 同步开机自启注册。
///
/// 已开启：每次启动都重新注册，使自启路径跟随当前运行的 exe
/// （版本升级或移动位置后，开机自启启动的是新程序而非老程序）。
/// 未开启：仅首次运行时默认开启；用户在界面关闭过则尊重其选择。
fn sync_autostart(app: &AppHandle) {
    if app.autolaunch().is_enabled().unwrap_or(false) {
        let _ = app.autolaunch().enable();
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    let flag = dir.join("initialized");
    if flag.exists() {
        return;
    }
    let _ = app.autolaunch().enable();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(flag, b"");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    remove_mark_of_the_web();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二个实例启动时聚焦已有窗口，不重复运行
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(monitor::new_shared())
        .invoke_handler(tauri::generate_handler![
            get_wifi_list,
            connect_wifi,
            get_connection_info,
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
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .menu(&menu)
                .tooltip("Auto WiFi Connector")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => show_main_window(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键单击托盘图标显示窗口
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
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

            sync_autostart(app.handle());

            monitor::spawn(app.handle().clone(), app.state::<Shared>().inner().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
