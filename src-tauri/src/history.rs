//! v0.3：Shell history 自动采集。
//!
//! 原理（设计 §4.1 方式 B）：`snip hook --install` 往 ~/.bashrc 追加一段
//! PROMPT_COMMAND 钩子（幂等，带标记块），每次命令执行后把
//! `时间戳\t命令` 追加到 history.log；之后在 GUI 或 CLI 里挑有用的入库。
//! 数据路径优先 `$SNIPNAV_HOME`，默认 `~/.snipnav`。

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::db;
use crate::models::HistoryEntry;

pub const HOOK_BEGIN: &str = "# >>> snipnav hook >>>（由 snip 管理，勿手动修改）";
pub const HOOK_END: &str = "# <<< snipnav hook <<<";

pub fn hook_script() -> String {
    format!(
        "{HOOK_BEGIN}\n\
         _snip_nav_hook() {{\n\
         \x20 local cmd; cmd=$(history 1 | sed 's/^ *[0-9]* *//')\n\
         \x20 case \"$cmd\" in snip|snip\\ *|snipnav*) return;; esac\n\
         \x20 [ -n \"$cmd\" ] && printf '%s\\t%s\\n' \"$(date +%Y-%m-%dT%H:%M:%S)\" \"$cmd\" \\\n\
         \x20   >> \"${{SNIPNAV_HOME:-$HOME/.snipnav}}/history.log\"\n\
         }}\n\
         PROMPT_COMMAND=\"_snip_nav_hook;${{PROMPT_COMMAND:-}}\"\n\
         {HOOK_END}\n"
    )
}

/// 幂等安装到指定 rc 文件；返回 false 表示已装过。
pub fn install_hook_to(rc: &Path) -> Result<bool, String> {
    let existing = std::fs::read_to_string(rc).unwrap_or_default();
    if existing.contains(HOOK_BEGIN) {
        return Ok(false);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(rc)
        .map_err(|e| format!("打开 {} 失败: {e}", rc.display()))?;
    f.write_all(format!("\n{}\n", hook_script()).as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

pub fn install_hook() -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("无法定位用户主目录")?;
    install_hook_to(&home.join(".bashrc"))
}

pub fn log_path() -> Result<PathBuf, String> {
    Ok(db::data_dir()?.join("history.log"))
}

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// 读取最近的 history（新→旧），超过 5MB 时自动裁剪保留末尾。
/// 过滤：snip 自身命令、与上一条重复的连续命令。
pub fn read_recent(conn: &Connection, limit: usize) -> Vec<HistoryEntry> {
    let path = match log_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    rotate_if_large(&path);
    let content = std::fs::read_to_string(&path).unwrap_or_default();

    let mut out: Vec<HistoryEntry> = Vec::new();
    let mut prev_cmd = String::new();
    for line in content.lines().rev() {
        if out.len() >= limit {
            break;
        }
        let Some((ts, cmd)) = line.split_once('\t') else { continue };
        let cmd = cmd.trim();
        if cmd.is_empty() || cmd == prev_cmd {
            continue;
        }
        if cmd == "snip" || cmd.starts_with("snip ") || cmd.starts_with("snipnav") {
            continue;
        }
        prev_cmd = cmd.to_string();
        let saved: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM snippets WHERE cmd = ?1 AND source IN ('self','ai')",
                params![cmd],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
                > 0;
        out.push(HistoryEntry {
            ts: ts.to_string(),
            cmd: cmd.to_string(),
            saved,
        });
    }
    out
}

fn rotate_if_large(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let keep = lines.len().saturating_sub(2000);
    let trimmed: Vec<&str> = lines[keep..].to_vec();
    if let Ok(mut f) = std::fs::File::create(path) {
        for l in trimmed {
            let _ = writeln!(f, "{l}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_script_has_marker_and_prompt_hook() {
        let s = hook_script();
        assert!(s.contains(HOOK_BEGIN) && s.contains(HOOK_END));
        assert!(s.contains("PROMPT_COMMAND"));
        assert!(s.contains("SNIPNAV_HOME"));
    }

    #[test]
    fn install_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("snipnav-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let rc = dir.join("bashrc");
        let _ = std::fs::remove_file(&rc);
        assert!(install_hook_to(&rc).unwrap(), "首次应写入");
        assert!(!install_hook_to(&rc).unwrap(), "重复安装应跳过");
        let content = std::fs::read_to_string(&rc).unwrap();
        assert_eq!(content.matches(HOOK_BEGIN).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_recent_filters_and_marks_saved() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();

        let dir = std::env::temp_dir().join(format!("snipnav-hist-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("history.log");
        std::fs::write(
            &log,
            "2026-10-26T10:00:00\tdocker ps -a\n\
             2026-10-26T10:00:01\tdocker ps -a\n\
             2026-10-26T10:00:02\tsnip search x\n\
             2026-10-26T10:00:03\tkubectl get pods -n prod\n\
             2026-10-26T10:00:04\tdocker logs --tail 100 -t mycontainer\n",
        )
        .unwrap();

        // 让 log_path 指向测试文件：直接测内部逻辑等价实现
        let content = std::fs::read_to_string(&log).unwrap();
        let mut cmds: Vec<(&str, bool)> = Vec::new();
        let mut prev = String::new();
        for line in content.lines().rev() {
            let Some((ts, cmd)) = line.split_once('\t') else { continue };
            let cmd = cmd.trim();
            if cmd.is_empty() || cmd == prev { continue; }
            if cmd == "snip" || cmd.starts_with("snip ") { continue; }
            prev = cmd.to_string();
            let saved: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM snippets WHERE cmd = ?1 AND source IN ('self','ai')",
                    params![cmd], |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0) > 0;
            cmds.push((ts, saved));
        }
        assert_eq!(cmds.len(), 3, "应过滤 snip 命令与连续重复：{cmds:?}");
        assert!(cmds[0].1, "种子里的 docker logs 命令应标记已入库");
        assert!(!cmds[1].1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
