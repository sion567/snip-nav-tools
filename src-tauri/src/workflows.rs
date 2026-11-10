//! v1.9：导入生态——Warp Workflows / Espanso 包 / VS Code snippets。
//!
//! 一个入口 `import_path` 自动识别格式：
//! - .json → VS Code snippets（对象 → {prefix, body, description}）
//! - .yaml/.yml → 顶层有 matches → Espanso 包；有 command → Warp Workflow
//! - 目录 → 递归扫描全部 .json/.yaml/.yml 逐文件识别
//!
//! 导入为 source='self' 的个人记录（用户主动挑选的内容），notes 标注来源，
//! cmd 去重（库里有同命令就跳过）；Warp/Espanso 的 {{var}} 占位符与
//! SnipNav 占位符填空语法天然同形，VS Code 的 $1/${1:默认} 转成 <1>/<默认>。

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Deserialize;

use crate::db::{self, NewSnippet};
use crate::models::WorkflowImportResult;
use crate::tagging;

pub struct ImportStat {
    pub files: i64,
    pub snippets: i64,
    pub skipped: i64,
}

/// 入口：文件或目录（递归）。
pub fn import_path(conn: &Connection, path: &Path) -> Result<WorkflowImportResult, String> {
    if !path.exists() {
        return Err(format!("路径不存在: {}", path.display()));
    }
    let mut stat = ImportStat { files: 0, snippets: 0, skipped: 0 };
    if path.is_file() {
        import_file(conn, path, &mut stat)?;
    } else {
        import_dir(conn, path, &mut stat)?;
    }
    Ok(WorkflowImportResult {
        files: stat.files,
        snippets: stat.snippets,
        skipped: stat.skipped,
    })
}

fn import_dir(conn: &Connection, dir: &Path, stat: &mut ImportStat) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {e}"))?;
    let mut children: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .collect();
    children.sort();
    for child in children {
        if child.is_dir() {
            import_dir(conn, &child, stat)?;
        } else if is_supported(&child) {
            import_file(conn, &child, stat)?;
        }
    }
    Ok(())
}

fn is_supported(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
        Some("json") | Some("yaml") | Some("yml")
    )
}

/// 单文件：按扩展名 + 内容特征识别格式。
fn import_file(conn: &Connection, path: &Path, stat: &mut ImportStat) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let before = (stat.snippets, stat.skipped);
    if ext == "json" {
        import_vscode(conn, &content, stat);
    } else if sniff_espanso(&content) {
        import_espanso(conn, &content, stat);
    } else {
        import_warp(conn, &content, stat);
    }
    if stat.snippets != before.0 || stat.skipped != before.1 {
        stat.files += 1;
    }
    Ok(())
}

/// 顶层有 matches 键 → Espanso。
fn sniff_espanso(yaml: &str) -> bool {
    serde_yaml::from_str::<EspansoFile>(yaml)
        .map(|f| !f.matches.is_empty())
        .unwrap_or(false)
}

// ---------- Warp Workflows（一文件一工作流） ----------

#[derive(Deserialize)]
struct WarpWorkflow {
    name: Option<String>,
    command: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    description: Option<String>,
}

fn import_warp(conn: &Connection, yaml: &str, stat: &mut ImportStat) {
    let Ok(wf) = serde_yaml::from_str::<WarpWorkflow>(yaml) else {
        stat.skipped += 1;
        return;
    };
    let Some(cmd) = wf.command.as_deref().map(str::trim).filter(|c| !c.is_empty()) else {
        stat.skipped += 1;
        return;
    };
    let intent = wf
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| wf.name.clone())
        .unwrap_or_else(|| cmd.to_string());
    if insert_import(conn, &intent, cmd, &wf.tags, "[Warp]") {
        stat.snippets += 1;
    } else {
        stat.skipped += 1;
    }
}

// ---------- Espanso 包（matches: [{trigger, replace}]） ----------

#[derive(Deserialize)]
struct EspansoFile {
    #[serde(default)]
    matches: Vec<EspansoMatch>,
}

#[derive(Deserialize)]
struct EspansoMatch {
    trigger: Option<String>,
    replace: Option<String>,
}

fn import_espanso(conn: &Connection, yaml: &str, stat: &mut ImportStat) {
    let Ok(file) = serde_yaml::from_str::<EspansoFile>(yaml) else {
        stat.skipped += 1;
        return;
    };
    for m in file.matches {
        let Some(replace) = m.replace.as_deref().map(str::trim).filter(|r| !r.is_empty()) else {
            continue;
        };
        let trigger = m.trigger.as_deref().unwrap_or("").trim();
        let intent = if trigger.is_empty() {
            replace.to_string()
        } else {
            format!("（espanso 触发词 {trigger}）{replace}")
        };
        if insert_import(conn, &intent, replace, &["espanso".to_string()], "[Espanso]") {
            stat.snippets += 1;
        } else {
            stat.skipped += 1;
        }
    }
}

// ---------- VS Code snippets（对象 → {prefix, body, description}） ----------

fn import_vscode(conn: &Connection, json: &str, stat: &mut ImportStat) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        stat.skipped += 1;
        return;
    };
    let Some(map) = value.as_object() else {
        stat.skipped += 1;
        return;
    };
    for (name, item) in map {
        let Some(body) = body_to_string(item.get("body")) else { continue };
        let description = item.get("description").and_then(|d| d.as_str()).unwrap_or("").trim();
        let prefix = item.get("prefix").and_then(|p| p.as_str()).unwrap_or("").trim();
        let intent = if description.is_empty() {
            name.clone()
        } else {
            format!("{description}（vscode 触发词 {prefix}）")
        };
        if insert_import(conn, &intent, &body, &["vscode".to_string()], "[VS Code]") {
            stat.snippets += 1;
        } else {
            stat.skipped += 1;
        }
    }
}

/// body：字符串数组（多行）或单字符串。$1 → <1>，${1:默认} → <默认>，$0 丢弃。
fn body_to_string(v: Option<&serde_json::Value>) -> Option<String> {
    let raw = match v? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            let mut lines = Vec::new();
            for item in arr {
                match item {
                    serde_json::Value::String(s) => lines.push(s.clone()),
                    _ => return None,
                }
            }
            lines.join("\n")
        }
        _ => return None,
    };
    if raw.trim().is_empty() {
        return None;
    }
    // ${name:default} → <default>（含占位嵌套到一层默认值）；$n → <n>；$$、$0 清理
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'{' {
                if let Some(end) = raw[i + 2..].find('}') {
                    let inner = &raw[i + 2..i + 2 + end];
                    let default = inner.split_once(':').map(|(_, d)| d).unwrap_or(inner);
                    out.push('<');
                    out.push_str(default);
                    out.push('>');
                    i += 2 + end + 1;
                    continue;
                }
            }
            if bytes[i + 1].is_ascii_digit() {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let n = &raw[i + 1..j];
                if n != "0" {
                    out.push('<');
                    out.push_str(n);
                    out.push('>');
                }
                i = j;
                continue;
            }
        }
        // 按 UTF-8 字符推进（避免切在多字节中间）
        let ch_len = raw[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&raw[i..i + ch_len]);
        i += ch_len;
    }
    // $0 等清理后可能留下行尾空白
    Some(out.trim_end().to_string())
}

/// 入库（source='self'，notes 标来源）；同命令已存在时返回 false（跳过）。
fn insert_import(conn: &Connection, intent: &str, cmd: &str, tags: &[String], source_note: &str) -> bool {
    let exists: i64 = conn
        .query_row("SELECT 1 FROM snippets WHERE cmd = ?1 LIMIT 1", params![cmd], |r| r.get(0))
        .unwrap_or(0);
    if exists == 1 {
        return false;
    }
    let (tool, command) = tagging::derive_tool_command(cmd);
    let merged = tagging::merge_tags(Some(tags), cmd, intent);
    let notes = format!("{source_note} 导入");
    let Ok(id) = db::insert_snippet(
        conn,
        NewSnippet {
            intent,
            cmd,
            project: None,
            tool: tool.as_deref(),
            command: command.as_deref(),
            variant_of: None,
            source: "self",
            verified: false,
            used_count: 0,
            last_used: None,
            notes: Some(&notes),
        },
    ) else {
        return false;
    };
    let _ = db::set_tags(conn, id, &merged);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn imports_warp_yaml() {
        let conn = setup();
        let yaml = "name: Docker Logs Tail\ncommand: docker logs --tail {{n}} {{c}}\ntags:\n  - docker\ndescription: 尾部查看容器日志\n";
        let mut stat = ImportStat { files: 0, snippets: 0, skipped: 0 };
        import_warp(&conn, yaml, &mut stat);
        assert_eq!(stat.snippets, 1);
        let (intent, cmd): (String, String) = conn
            .query_row(
                "SELECT intent, cmd FROM snippets WHERE notes = '[Warp] 导入'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cmd, "docker logs --tail {{n}} {{c}}");
        assert_eq!(intent, "尾部查看容器日志");
    }

    #[test]
    fn imports_espanso_matches() {
        let conn = setup();
        let yaml = "matches:\n  - trigger: ;dl\n    replace: docker logs -f {{c}}\n  - trigger: ;x\n    replace: \"\"\n";
        let mut stat = ImportStat { files: 0, snippets: 0, skipped: 0 };
        import_espanso(&conn, yaml, &mut stat);
        assert_eq!(stat.snippets, 1, "空 replace 跳过");
        let intent: String = conn
            .query_row("SELECT intent FROM snippets WHERE notes = '[Espanso] 导入'", [], |r| r.get(0))
            .unwrap();
        assert!(intent.contains(";dl"), "意图应含触发词: {intent}");
        assert!(sniff_espanso(yaml), "espanso 识别");
        assert!(!sniff_espanso("name: x\ncommand: y"), "warp 不误判为 espanso");
    }

    #[test]
    fn imports_vscode_json_and_converts_placeholders() {
        let conn = setup();
        let json = r#"{
            "打包前端": {
                "prefix": "pnpm-build",
                "body": ["pnpm build", "echo done $1 and ${2:dist} $$ final $0"],
                "description": "执行前端打包"
            }
        }"#;
        let mut stat = ImportStat { files: 0, snippets: 0, skipped: 0 };
        import_vscode(&conn, json, &mut stat);
        assert_eq!(stat.snippets, 1);
        let cmd: String = conn
            .query_row("SELECT cmd FROM snippets WHERE notes = '[VS Code] 导入'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cmd, "pnpm build\necho done <1> and <dist> $ final");
    }

    #[test]
    fn dedupes_by_cmd_and_path_entry() {
        let conn = setup();
        let dir = std::env::temp_dir().join(format!("snipnav-wf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("w.yaml"),
            "name: A\ncommand: docker ps -a\ndescription: 列出容器\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.json"), r#"{"B": {"prefix": "x", "body": ["docker ps -a"]}}"#).unwrap();
        std::fs::write(dir.join("ignore.txt"), "not supported").unwrap();

        let r1 = import_path(&conn, &dir).unwrap();
        assert_eq!(r1.files, 2, "txt 不计入");
        assert_eq!(r1.snippets, 1, "同命令第二条应跳过");
        assert_eq!(r1.skipped, 1);

        // 重复导入整个目录：全部按命令去重
        let r2 = import_path(&conn, &dir).unwrap();
        assert_eq!(r2.snippets, 0);
        assert_eq!(r2.skipped, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
