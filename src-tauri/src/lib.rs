//! SnipNav —— 本地命令速查与智能发散（Tauri 桌面应用）。

pub mod commands;
pub mod db;
pub mod docs;
pub mod explore;
pub mod helpgen;
pub mod history;
pub mod importer;
pub mod llm;
pub mod models;
pub mod search;
pub mod seeds;
pub mod share;
pub mod tagging;

use std::sync::Mutex;

use tauri::Manager;

/// 全局数据库状态：单连接 + 互斥锁（本地单窗口场景足够）。
pub struct Db(pub Mutex<rusqlite::Connection>);

pub fn run_gui() {
    let conn = db::open().expect("打开数据库失败");
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(Db(Mutex::new(conn.into_inner())))
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::get_snippet,
            commands::create_snippet,
            commands::update_snippet,
            commands::delete_snippet,
            commands::copy_snippet,
            commands::copy_variant,
            commands::explore,
            commands::explore_save,
            commands::stats,
            commands::tags,
            commands::import_tldr,
            commands::generate_params,
            commands::get_doc_body,
            commands::copy_to_clipboard,
            commands::recent_history,
            commands::llm_suggest,
            commands::get_settings,
            commands::save_settings,
            commands::autostart_status,
            commands::autostart_enable,
            commands::autostart_disable
        ])
        .setup(|app| {
            // v1.x 托盘：关闭窗口隐藏到托盘，托盘菜单可显示/退出
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            let show = MenuItem::with_id(app, "show", "显示 SnipNav", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
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
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
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
