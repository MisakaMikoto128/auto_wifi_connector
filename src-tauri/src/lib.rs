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

/// 界面设置，持久化到配置目录的 settings.json。
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Settings {
    /// 自动断网重连轮询开关。
    #[serde(default)]
    auto_reconnect: bool,
}

fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

/// 读取设置；文件不存在或损坏时返回默认值。
fn load_settings(app: &AppHandle) -> Settings {
    let Some(path) = settings_path(app) else {
        return Settings::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    let Some(path) = settings_path(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(settings) {
        let _ = std::fs::write(path, text);
    }
}

/// 启用或停用自动断网重连轮询（同时持久化到 settings.json）。
#[tauri::command]
fn set_auto_reconnect(app: AppHandle, state: State<'_, Shared>, enabled: bool) -> MonitorSnapshot {
    let snapshot = {
        let mut guard = state.lock().unwrap();
        guard.enabled = enabled;
        guard.clone()
    };
    save_settings(
        &app,
        &Settings {
            auto_reconnect: enabled,
        },
    );
    let _ = app.emit("monitor", &snapshot);
    snapshot
}

/// 在系统浏览器中打开开源仓库页面。
#[tauri::command]
fn open_repo() {
    #[cfg(windows)]
    unsafe {
        use windows::core::w;
        let _ = windows::Win32::UI::Shell::ShellExecuteW(
            None,
            w!("open"),
            w!("https://github.com/MisakaMikoto128/auto_wifi_connector"),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
    #[cfg(not(windows))]
    {
        const URL: &str = "https://github.com/MisakaMikoto128/auto_wifi_connector";
        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(target_os = "linux")]
        let opener = "xdg-open";
        let _ = std::process::Command::new(opener).arg(URL).spawn();
    }
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
/// 此后开机自启不再出现该提示。仅 Windows 存在此机制。
#[cfg(windows)]
fn remove_mark_of_the_web() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let ads = format!("{}:Zone.Identifier", exe.display());
    let _ = std::fs::remove_file(ads);
}

#[cfg(not(windows))]
fn remove_mark_of_the_web() {}

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
            open_repo,
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

            // 恢复上次退出时的自动重连开关状态
            let shared = app.state::<Shared>();
            shared.lock().unwrap().enabled = load_settings(app.handle()).auto_reconnect;

            monitor::spawn(app.handle().clone(), shared.inner().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let s = Settings {
            auto_reconnect: true,
        };
        let text = serde_json::to_string(&s).unwrap();
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert!(parsed.auto_reconnect);
    }

    #[test]
    fn settings_default_on_missing_or_corrupt() {
        let parsed: Settings = serde_json::from_str("{}").unwrap_or_default();
        assert!(!parsed.auto_reconnect);
        let corrupt: Settings = serde_json::from_str("not json").unwrap_or_default();
        assert!(!corrupt.auto_reconnect);
    }
}
