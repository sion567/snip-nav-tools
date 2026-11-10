//! v1.6：剪贴板监听（Raycast/Ditto 式「顺手入库」）。
//!
//! 后台线程每 2 秒读一次剪贴板；新内容像一条命令（首 token 命中已知工具名）、
//! 单行、库里还没有同命令记录时，记为「待入库」，主窗口横幅提醒。
//! 默认关闭（settings: clipboard_watch = '1' 开启），开关每轮从库里读，改设置即时生效。
//! 完全本地：只读剪贴板文本，不联网、不自动入库（存不存由用户点横幅决定）。

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db;

/// 全局「待入库」状态：监听线程写，IPC 读/清。
pub struct Pending(pub Mutex<Option<PendingCmd>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCmd {
    pub cmd: String,
    pub ts: String,
}

pub const KEY_CLIPBOARD_WATCH: &str = "clipboard_watch";

/// 内置已知工具名（首 token 命中才算命令）；库里出现过的 tool 会动态并入。
pub const BUILTIN_TOOLS: &[&str] = &[
    "docker", "git", "kubectl", "helm", "npm", "pnpm", "yarn", "node", "npx",
    "cargo", "rustc", "gradle", "gradlew", "mvn", "java", "javac",
    "curl", "wget", "ssh", "scp", "rsync", "ping", "nslookup",
    "python", "python3", "pip", "pip3", "go", "sqlite3", "mysql", "psql", "redis-cli",
    "grep", "awk", "sed", "tar", "zip", "unzip", "find", "xargs", "chmod", "chown",
    "netstat", "ipconfig", "tasklist", "taskkill", "netsh", "robocopy", "wsl",
    "ffmpeg", "ffprobe", "systemctl", "apt", "brew", "choco", "winget",
    "terraform", "aws", "az", "gcloud", "shutdown", "sfc", "dism",
];

/// 是否像一条可入库的命令：单行、长度合理、过滤 snip 自身、首 token 命中已知工具。
/// 返回清理后的命令文本。
pub fn looks_like_command(text: &str, tools: &HashSet<String>) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.len() > 500 || t.contains('\n') {
        return None;
    }
    if t == "snip" || t.starts_with("snip ") || t.starts_with("snipnav") {
        return None;
    }
    // 纯数字 / 短横线等明显不是命令
    let first_raw = t.split_whitespace().next()?;
    if first_raw.len() < 2 {
        return None;
    }
    let first = first_raw
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .to_lowercase();
    let first = match first.as_str() {
        "gradlew" => "gradle".to_string(),
        other => other.to_string(),
    };
    tools.contains(&first).then(|| t.to_string())
}

/// 已知工具名 = 内置表 ∪ 库里出现过的 tool（走 idx_snippets_tool）。
fn known_tools(conn: &Connection) -> HashSet<String> {
    let mut set: HashSet<String> = BUILTIN_TOOLS.iter().map(|s| s.to_string()).collect();
    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT tool FROM snippets WHERE tool IS NOT NULL AND tool != ''",
    ) else {
        return set;
    };
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for t in rows {
        set.insert(t.to_lowercase());
    }
    set
}

/// 后台监听线程。设计要点：
/// - 开关每轮读 settings（改设置即时生效，无需重启）
/// - 首轮只记基线不触发（应用启动时剪贴板里已有的东西不弹）
/// - 只在剪贴板**变化**时评估一次， dismissing 后同内容不重复弹
pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_text: Option<String> = None;
        let mut cb: Option<arboard::Clipboard> = None;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let Some(db_state) = app.try_state::<crate::Db>() else { continue };
            let enabled = {
                let Ok(conn) = db_state.0.lock() else { continue };
                db::get_setting(&conn, KEY_CLIPBOARD_WATCH)
                    .map(|v| v.trim() == "1")
                    .unwrap_or(false)
            };
            if !enabled {
                continue;
            }
            if cb.is_none() {
                cb = arboard::Clipboard::new().ok();
            }
            let Some(clip) = cb.as_mut() else { continue };
            let Ok(text) = clip.get_text() else { continue };
            if last_text.is_none() {
                last_text = Some(text);
                continue;
            }
            if last_text.as_deref() == Some(text.as_str()) {
                continue;
            }
            last_text = Some(text.clone());

            let Some(cmd) = (|| {
                let conn = db_state.0.lock().ok()?;
                let tools = known_tools(&conn);
                let cmd = looks_like_command(&text, &tools)?;
                // 已入库的同命令不再提醒
                let exists: i64 = conn
                    .query_row(
                        "SELECT 1 FROM snippets WHERE cmd = ?1 LIMIT 1",
                        params![cmd],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if exists == 1 {
                    return None;
                }
                Some(cmd)
            })() else {
                continue;
            };

            if let Some(pending) = app.try_state::<Pending>() {
                if let Ok(mut p) = pending.0.lock() {
                    *p = Some(PendingCmd {
                        cmd,
                        ts: chrono::Local::now().format("%H:%M:%S").to_string(),
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> HashSet<String> {
        BUILTIN_TOOLS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recognizes_command_like_text() {
        assert_eq!(
            looks_like_command("docker logs -f web", &tools()).as_deref(),
            Some("docker logs -f web")
        );
        assert_eq!(
            looks_like_command("  git status --short  ", &tools()).as_deref(),
            Some("git status --short"),
            "首尾空白应清理"
        );
        // ./gradlew / gradlew 归一为 gradle
        assert!(looks_like_command("./gradlew bootRun", &tools()).is_some());
    }

    #[test]
    fn rejects_non_command_text() {
        assert_eq!(looks_like_command("hello world", &tools()), None, "首 token 非工具名");
        assert_eq!(looks_like_command("", &tools()), None);
        assert_eq!(looks_like_command("line1\nline2", &tools()), None, "多行不要");
        assert_eq!(looks_like_command("snip search x", &tools()), None, "过滤自身");
        assert_eq!(looks_like_command("https://example.com/a/b", &tools()), None);
        assert_eq!(looks_like_command("-42", &tools()), None, "短 token 不要");
    }

    #[test]
    fn dynamic_tools_extend_builtin() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        db::insert_snippet(
            &conn,
            db::NewSnippet {
                intent: "自研工具",
                cmd: "mytool deploy --env prod",
                project: None,
                tool: Some("mytool"),
                command: None,
                variant_of: None,
                source: "self",
                verified: true,
                used_count: 0,
                last_used: None,
                notes: None,
            },
        )
        .unwrap();
        let set = known_tools(&conn);
        assert!(set.contains("mytool"), "库里的 tool 应并入识别表");
        assert!(set.contains("docker"), "内置工具应保留");
    }
}
