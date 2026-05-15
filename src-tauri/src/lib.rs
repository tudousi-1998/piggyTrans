mod selection;
mod settings;
mod translate;

#[cfg(target_os = "macos")]
mod macos_overlay;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayOpenPayload {
    mode: String,
    text: Option<String>,
    anchor_near_cursor: bool,
}

fn fetch_selection_on_main_thread() -> selection::SelectionOutcome {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(selection::fetch_selection))
        .unwrap_or(selection::SelectionOutcome::Empty)
}

fn is_settings_window_open(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("settings")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

async fn handle_translate_hotkey(app: tauri::AppHandle) {
    if is_settings_window_open(&app) {
        return;
    }

    // macOS 上 Accessibility / 模拟按键必须在主线程执行，否则可能崩溃。
  let (tx, rx) = std::sync::mpsc::sync_channel(1);
  if let Err(e) = app.run_on_main_thread(move || {
    let outcome = fetch_selection_on_main_thread();
    let _ = tx.send(outcome);
  }) {
    eprintln!("piggytrans: schedule selection on main thread failed: {e}");
  }
  let outcome = rx
    .recv()
    .unwrap_or(selection::SelectionOutcome::Empty);

    let payload = match outcome {
        selection::SelectionOutcome::PermissionDenied => OverlayOpenPayload {
            mode: "permission".into(),
            text: None,
            anchor_near_cursor: true,
        },
        selection::SelectionOutcome::Empty => OverlayOpenPayload {
            mode: "manual".into(),
            text: None,
            anchor_near_cursor: true,
        },
        selection::SelectionOutcome::Text(t) => OverlayOpenPayload {
            mode: "translate".into(),
            text: Some(t),
            anchor_near_cursor: true,
        },
    };

    let payload_for_ui = payload.clone();
    let app_for_ui = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        if let Err(err) = present_overlay(&app_for_ui, &payload_for_ui) {
            eprintln!("piggytrans: present overlay failed: {err}");
        }
    }) {
        eprintln!("piggytrans: schedule overlay on main thread failed: {e}");
    }
}

fn present_overlay(app: &tauri::AppHandle, payload: &OverlayOpenPayload) -> Result<(), String> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or_else(|| "overlay 窗口不存在".to_string())?;

    let was_visible = overlay.is_visible().unwrap_or(false);

    #[cfg(target_os = "macos")]
    {
        let ns_window = overlay
            .ns_window()
            .map_err(|e| e.to_string())?;
        // 关键：在 orderFront 之前禁用 Tauri 的 CanJoinAllSpaces（与 MoveToActiveSpace 冲突），
        // 然后由 native 层重设 collection behavior + level + 抬升。
        let _ = overlay.set_visible_on_all_workspaces(false);
        macos_overlay::configure(ns_window);
        if !was_visible {
            macos_overlay::place_near_mouse(ns_window);
        }
        macos_overlay::activate_and_order_front(ns_window);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = overlay.set_visible_on_all_workspaces(true);
        let _ = overlay.set_always_on_top(true);
        if !was_visible {
            position_overlay_near_cursor(app, &overlay)?;
            overlay.show().map_err(|e| e.to_string())?;
            let _ = overlay.unminimize();
        }
        let _ = overlay.set_focus();
    }

    overlay
        .emit("piggy-open", payload)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn position_overlay_near_cursor(
    app: &tauri::AppHandle,
    overlay: &tauri::WebviewWindow,
) -> Result<(), String> {
    use tauri::{PhysicalPosition, Position};

    let cursor = overlay
        .cursor_position()
        .or_else(|_| app.cursor_position())
        .map_err(|e| e.to_string())?;
    let mx = cursor.x;
    let my = cursor.y;

    let size = overlay.outer_size().map_err(|e| e.to_string())?;
    let w = size.width as f64;
    let h = size.height as f64;
    let margin = 12.0;

    let monitor = overlay
        .monitor_from_point(mx, my)
        .ok()
        .flatten()
        .or_else(|| overlay.current_monitor().ok().flatten())
        .or_else(|| overlay.primary_monitor().ok().flatten());

    let (mut px, mut py) = (mx - w / 2.0, my + margin);

    if let Some(mon) = monitor {
        let area = mon.work_area();
        let vf_x = area.position.x as f64;
        let vf_y = area.position.y as f64;
        let vf_max_x = vf_x + area.size.width as f64;
        let vf_max_y = vf_y + area.size.height as f64;

        if px + w > vf_max_x - margin {
            px = mx - w - margin;
        }
        if px < vf_x + margin {
            px = vf_x + margin;
        }
        if py + h > vf_max_y - margin {
            py = my - h - margin;
        }
        if py < vf_y + margin {
            py = vf_y + margin;
        }

        let max_x = (vf_max_x - w - margin).max(vf_x + margin);
        let max_y = (vf_max_y - h - margin).max(vf_y + margin);
        px = px.clamp(vf_x + margin, max_x);
        py = py.clamp(vf_y + margin, max_y);
    } else {
        px = px.max(8.0);
        py = py.max(8.0);
    }

    overlay
        .set_position(Position::Physical(PhysicalPosition::new(
            px.round() as i32,
            py.round() as i32,
        )))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn register_hotkey(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(hotkey, move |h, _, event| {
        if event.state != ShortcutState::Pressed {
            return;
        }
        if is_settings_window_open(&h) {
            return;
        }
        let hh = h.clone();
        tauri::async_runtime::spawn(async move {
            handle_translate_hotkey(hh).await;
        });
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn apply_autostart(app: &tauri::AppHandle, enable: bool) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use tauri_plugin_autostart::ManagerExt;
        if enable {
            app.autolaunch().enable().map_err(|e| e.to_string())?;
        } else {
            app.autolaunch().disable().map_err(|e| e.to_string())?;
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (app, enable);
    }
    Ok(())
}

#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<settings::AllSettings, String> {
    settings::load_all(&app)
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: settings::AllSettings) -> Result<(), String> {
    settings::save_all(&app, &settings)?;
    register_hotkey(&app, &settings.general.hotkey)?;
    apply_autostart(&app, settings.general.launch_at_login)?;
    Ok(())
}

#[tauri::command]
async fn translate(
    app: tauri::AppHandle,
    text: String,
) -> Result<translate::TranslationResult, String> {
    let all = settings::load_all(&app)?;
    translate::translate(&all, &text).await
}

#[tauri::command]
fn overlay_hide(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let urls = [
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.Settings.extension?privacy_security_accessibility",
        ];
        for u in urls {
            let _ = std::process::Command::new("open").arg(u).spawn();
        }
    }
    Ok(())
}

#[tauri::command]
fn request_ax_trust_prompt() -> Result<(), String> {
    selection::request_trust_prompt();
    Ok(())
}

#[tauri::command]
fn show_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        w.show().map_err(|e| e.to_string())?;
        let _ = w.set_focus();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    let mut autostart = tauri_plugin_autostart::Builder::new();
    #[cfg(target_os = "macos")]
    {
        autostart = autostart.macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent);
    }
    builder = builder.plugin(autostart.build());

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                // 必须在创建任何窗口之前切到 .accessory，否则窗口被锁在 regular 策略
                // 对应的 Space，无法跟随用户进入其他 app 全屏 Space。
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                macos_overlay::set_accessory_activation_policy();
            }

            // accessory 策略生效后，再按 config 创建窗口（overlay/settings 都标了 create:false）
            let configs: Vec<_> = app.config().app.windows.clone();
            for cfg in &configs {
                tauri::WebviewWindowBuilder::from_config(app.handle(), cfg)?.build()?;
            }

            let handle = app.handle().clone();
            let settings_i = settings::load_all(&handle)
                .map(|a| a.general)
                .unwrap_or_default();
            register_hotkey(&handle, &settings_i.hotkey)?;
            let _ = apply_autostart(&handle, settings_i.launch_at_login);

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?,
                ],
            )?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        let _ = show_settings_window(app.clone());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }

            let _tray = tray.build(app)?;

            if let Some(overlay) = app.get_webview_window("overlay") {
                #[cfg(target_os = "macos")]
                if let Ok(ns_window) = overlay.ns_window() {
                    macos_overlay::configure(ns_window);
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = overlay.set_always_on_top(true);
                    let _ = overlay.set_visible_on_all_workspaces(true);
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(window.label(), "settings" | "overlay") {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // 点击弹窗外部自动关闭 overlay
            if window.label() == "overlay" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            translate,
            overlay_hide,
            open_accessibility_settings,
            request_ax_trust_prompt,
            show_settings_window,
        ])
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
