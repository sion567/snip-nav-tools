//! SnipNav —— 本地命令速查与智能发散（Tauri 桌面应用）。

pub mod clipboard;
pub mod commands;
pub mod db;
pub mod docs;
pub mod explore;
pub mod helpgen;
pub mod history;
pub mod importer;
pub mod llm;
pub mod models;
pub mod paste;
pub mod search;
pub mod seeds;
pub mod share;
pub mod sync;
pub mod tagging;
pub mod workflows;

use std::sync::Mutex;

use tauri::Manager;

/// 全局数据库状态：单连接 + 互斥锁（本地单窗口场景足够）。
pub struct Db(pub Mutex<rusqlite::Connection>);

/// v1.5 速查浮窗：可见则隐藏，否则显示并聚焦（全局快捷键 / 托盘菜单共用）。
/// 显示瞬间记下前台窗口（v1.8 粘贴回原窗口的目标）。
pub fn toggle_quick(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("quick") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            if let Some(pt) = app.try_state::<paste::PasteTarget>() {
                paste::capture_target(&pt);
            }
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

/// v1.5 注册速查浮窗的全局快捷键（先注销旧键再注册新键）。
/// 空串 = 禁用快捷键。格式示例："Alt+Space" / "Ctrl+Shift+P"。
pub fn register_quick_hotkey(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let h = hotkey.trim();
    if h.is_empty() {
        return Ok(());
    }
    let sc: tauri_plugin_global_shortcut::Shortcut = h
        .parse()
        .map_err(|_| format!("无法解析快捷键「{h}」，示例：Alt+Space 或 Ctrl+Shift+P"))?;
    gs.register(sc)
        .map_err(|e| format!("注册快捷键「{h}」失败（可能被其他程序占用）：{e}"))
}

pub fn run_gui() {
    let conn = db::open().expect("打开数据库失败");
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // 按下即触发（与 Raycast 习惯一致；松开不动作）
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        toggle_quick(app);
                    }
                })
                .build(),
        )
        .manage(Db(Mutex::new(conn.into_inner())))
        .manage(clipboard::Pending(Mutex::new(None)))
        .manage(paste::PasteTarget(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::browse_docs,
            commands::get_snippet,
            commands::create_snippet,
            commands::update_snippet,
            commands::delete_snippet,
            commands::set_pinned,
            commands::copy_snippet,
            commands::copy_variant,
            commands::explore,
            commands::explore_save,
            commands::stats,
            commands::tags,
            commands::app_version,
            commands::import_tldr,
            commands::generate_params,
            commands::get_doc_body,
            commands::copy_to_clipboard,
            commands::recent_history,
            commands::llm_suggest,
            commands::get_settings,
            commands::save_settings,
            commands::set_quick_hotkey,
            commands::quick_hide,
            commands::pending_clipboard,
            commands::dismiss_pending_clipboard,
            commands::playbooks,
            commands::playbook_save,
            commands::playbook_delete,
            commands::paste_and_hide,
            commands::heatmap,
            commands::import_workflows,
            commands::llm_models,
            commands::autostart_status,
            commands::autostart_enable,
            commands::autostart_disable
        ])
        .setup(|app| {
            // v1.x 托盘：关闭窗口隐藏到托盘，托盘菜单可显示/退出
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            let show = MenuItem::with_id(app, "show", "显示 SnipNav", true, None::<&str>)?;
            let quick = MenuItem::with_id(app, "quick", "⚡ 速查浮窗", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quick, &quit])?;
            TrayIconBuilder::with_id("snipnav-tray")
                .icon(app.default_window_icon().cloned().ok_or("缺少应用图标")?)
                .tooltip("SnipNav · 本地命令速查")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quick" => toggle_quick(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // v1.5 速查浮窗：注册全局快捷键（设置里可改，默认 Alt+Space）。
            // 注册失败（被占用/解析失败）不阻断启动，仅提示——应用其余功能不受影响。
            let hotkey = {
                let state = app.state::<Db>();
                let conn = state
                    .0
                    .lock()
                    .map_err(|e| format!("数据库锁异常: {e}"))?;
                // 用户显式设置过（含空串=禁用）就用设置值；从未设置才用默认
                match db::get_setting(&conn, commands::KEY_QUICK_HOTKEY) {
                    Some(v) => v,
                    None => "Alt+Space".into(),
                }
            };
            if let Err(e) = register_quick_hotkey(app.handle(), &hotkey) {
                eprintln!("全局快捷键注册失败（应用继续运行，可在设置里换一个）：{e}");
            }

            // v1.6 剪贴板监听（默认关闭，设置里开启；线程内部每轮读开关，改了即时生效）
            clipboard::spawn_watcher(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("SnipNav 构建失败")
        .run(|app, event| {
            // v1.2 P0-1：退出前把 WAL 合并回主库并截断，-wal 文件不遗留占盘
            //（托盘退出 app.exit(0) / 系统关机都会走到 RunEvent::Exit）
            if let tauri::RunEvent::Exit = event {
                if let Some(db) = app.try_state::<Db>() {
                    if let Ok(conn) = db.0.lock() {
                        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                    }
                }
            }
        });
}
