//! v1.1 性能优化包 ①/④：批量文档导入（man 等）——结构化入库。
//!
//! 原则（对应讨论的四条优化）：
//! - ① 可搜索的 snippets 只存小行（名称 + 一句简介）；整页原文进 `doc_bodies`
//!   旁表，点「📖 全文」时按 id 取，O(1)。
//! - ④ 旁表永不参与 LIKE 搜索、也不进 FTS 索引——百 MB 语料下搜索数据量不变。
//! - 选项段落解析进 param_tree（复用 helpgen 的参数段解析，兼容 man 的
//!   「flag 行 + 下一行缩进描述」与 --help 的「同行两空格」两种格式）。
//!
//! 语料来源：任何纯文本手册页。Linux/WSL 导出示例：`man -P cat docker-logs > docker-logs.txt`

use std::path::Path;

use rusqlite::{params, Connection};

use crate::db::{self, NewSnippet};
use crate::helpgen::{self, GeneratedParam};
use crate::models::ImportResult;
use crate::tagging;

/// 解析 man 页的 NAME 行：`       docker-logs - Fetch the logs of a container`
fn parse_name_brief(content: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_string();
    let mut brief = String::new();
    let mut in_name_section = false;
    for line in content.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("NAME") {
            in_name_section = true;
            continue;
        }
        if in_name_section {
            if t.is_empty() {
                continue;
            }
            if let Some((n, b)) = t.split_once(" - ") {
                name = n.trim().to_string();
                brief = b.trim().chars().take(100).collect();
            }
            break;
        }
        if !t.is_empty() && brief.is_empty() && !t.starts_with('(') && !t.contains("Manual") {
            brief = t.chars().take(100).collect();
        }
    }
    (name, brief)
}

/// man 格式选项：flag 行（缩进、无同行说明），描述在后续更缩进的行。
/// 形如：
/// ```text
///        -f, --follow
///               Follow log output
/// ```
fn parse_man_indented_options(text: &str) -> Vec<GeneratedParam> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<GeneratedParam> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();
    let indent_of = |s: &str| s.len() - s.trim_start().len();

    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim();
        let ind = indent_of(raw);
        // flag 行：深缩进、以 - 开头、短行、不含 ≥2 空格的同行说明
        if ind >= 6 && t.starts_with('-') && t.len() < 60 && !t.contains("  ") {
            // 跨行 flag（以 , 结尾的续行）
            let mut spec = t.to_string();
            let mut j = i + 1;
            while j < lines.len() {
                let nt = lines[j].trim();
                if indent_of(lines[j]) == ind && nt.ends_with(',') && nt.starts_with('-') {
                    spec.push(' ');
                    spec.push_str(nt);
                    j += 1;
                } else {
                    break;
                }
            }
            // 描述：更缩进的后续行，直到空行 / 回到 flag 缩进
            let mut desc = String::new();
            while j < lines.len() {
                let l = lines[j];
                let lt = l.trim();
                if lt.is_empty() {
                    break;
                }
                if indent_of(l) <= ind {
                    break;
                }
                if !desc.is_empty() {
                    desc.push(' ');
                }
                desc.push_str(lt);
                j += 1;
                if desc.len() > 120 {
                    break;
                }
            }
            if let Some(p) = helpgen::param_from_spec(&spec, desc) {
                let key = p.flag.rsplit(", ").next().unwrap_or(&p.flag).to_string();
                if seen.insert(key) {
                    out.push(p);
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// 合并两种格式的选项解析（同行两空格 + man 下一行缩进），按长参数去重，
/// 同一参数保留描述更完整的一版。
pub fn parse_page_options(content: &str) -> Vec<GeneratedParam> {
    let mut out: Vec<GeneratedParam> = Vec::new();
    for p in helpgen::parse_help(content)
        .into_iter()
        .chain(parse_man_indented_options(content))
    {
        let key = p.flag.rsplit(", ").next().unwrap_or(&p.flag).to_string();
        if let Some(prev) = out
            .iter_mut()
            .find(|q| q.flag.rsplit(", ").next().unwrap_or(&q.flag) == key)
        {
            if p.desc.len() > prev.desc.len() {
                *prev = p;
            }
        } else {
            out.push(p);
        }
    }
    out
}

fn walk_doc_files(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth > 5 || out.len() > 5000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if p.is_dir() {
            if !name.starts_with('.') {
                walk_doc_files(&p, out, depth + 1);
            }
        } else {
            let ext_ok = name.ends_with(".txt")
                || name.ends_with(".md")
                || name.ends_with(".1") || name.ends_with(".2") || name.ends_with(".3")
                || name.ends_with(".4") || name.ends_with(".5") || name.ends_with(".6")
                || name.ends_with(".7") || name.ends_with(".8");
            if ext_ok && !name.starts_with("readme") && name != "index.md" {
                out.push(p);
            }
        }
    }
}

/// 导入目录下的纯文本手册页：一页 → 一条 doc 记录 + 旁表全文 + 选项进参数树。
pub fn import_docs_dir(conn: &Connection, root: &Path) -> Result<ImportResult, String> {
    if !root.is_dir() {
        return Err(format!("目录不存在: {}", root.display()));
    }
    let mut files = Vec::new();
    walk_doc_files(root, &mut files, 0);
    let mut res = ImportResult { pages: 0, snippets: 0, skipped: 0 };

    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    for f in &files {
        let Ok(content) = std::fs::read_to_string(f) else { continue };
        let content = content.trim_start_matches('\u{feff}');
        if content.trim().is_empty() {
            continue;
        }
        // 文件名 → 命令名（docker-logs.1 / docker-logs.txt → docker logs）
        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let cmd_name = stem.replace('-', " ");
        let dup: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snippets WHERE source='doc' AND cmd = ?1",
                params![cmd_name],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if dup > 0 {
            res.skipped += 1;
            continue;
        }
        let (name, brief) = parse_name_brief(content, &stem);
        let intent = if brief.is_empty() {
            format!("{name} 手册页")
        } else {
            format!("{name}：{brief}")
        };
        let (tool, command) = tagging::derive_tool_command(&cmd_name);
        let id = match db::insert_snippet(
            conn,
            NewSnippet {
                intent: &intent,
                cmd: &cmd_name,
                project: None,
                tool: tool.as_deref(),
                command: command.as_deref(),
                variant_of: None,
                source: "doc",
                verified: false,
                used_count: 0,
                last_used: None,
                notes: Some("手册页：点「📖 全文」查看整页"),
            },
        ) {
            Ok(id) => id,
            Err(_) => {
                res.skipped += 1;
                continue;
            }
        };
        let _ = conn.execute(
            "INSERT OR REPLACE INTO doc_bodies(snippet_id, body) VALUES (?1, ?2)",
            params![id, content],
        );
        let tags = tagging::auto_tags(&cmd_name, &brief);
        let _ = db::set_tags(conn, id, &tags);

        // 选项段落 → 参数树（tool/command 与记录一致）
        let opts = parse_page_options(content);
        if !opts.is_empty() {
            let _ = helpgen::insert_params(conn, tool.as_deref().unwrap_or(""), command.as_deref().unwrap_or(""), &opts);
        }
        res.pages += 1;
        res.snippets += 1;
    }
    if let Err(e) = conn.execute_batch("COMMIT") {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(format!("提交导入事务失败: {e}"));
    }
    Ok(res)
}

pub fn get_body(conn: &Connection, snippet_id: i64) -> Option<String> {
    conn.query_row(
        "SELECT body FROM doc_bodies WHERE snippet_id = ?1",
        [snippet_id],
        |r| r.get(0),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAN_PAGE: &str = "\
DOCKER-LOGS(1)                User Commands               DOCKER-LOGS(1)

NAME
       docker-logs - Fetch the logs of a container

SYNOPSIS
       docker logs [OPTIONS] NAME

OPTIONS
       -f, --follow
              Follow log output

       --since string
              Show logs since timestamp (e.g. 42m)

       -t, --timestamps
              Show timestamps

       --tail string
              Number of lines to show from the end of the logs
";

    #[test]
    fn parses_name_brief_and_indented_options() {
        let (name, brief) = parse_name_brief(MAN_PAGE, "fallback");
        assert_eq!(name, "docker-logs");
        assert!(brief.contains("Fetch the logs"));

        let opts = parse_page_options(MAN_PAGE);
        let flags: Vec<&str> = opts.iter().map(|p| p.flag.as_str()).collect();
        assert!(flags.contains(&"-f, --follow"), "flags={flags:?}");
        assert!(flags.contains(&"--since"), "flags={flags:?}");
        assert!(flags.contains(&"--tail"), "flags={flags:?}");
        let since = opts.iter().find(|p| p.flag == "--since").unwrap();
        assert!(since.desc.contains("since timestamp"));
        assert_eq!(since.example, "--since <string>");
        assert_eq!(since.category, "time");
    }

    #[test]
    fn import_creates_doc_snippet_body_and_params() {
        let dir = std::env::temp_dir().join(format!("snipnav-docs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("docker-logs.1");
        std::fs::write(&file, MAN_PAGE).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();
        let before = db::count(&conn, "param_tree");

        let res = import_docs_dir(&conn, &dir).unwrap();
        assert_eq!((res.pages, res.snippets), (1, 1));
        // doc 记录：cmd 名规范化为 "docker logs"
        let (cmd, intent): (String, String) = conn
            .query_row("SELECT cmd, intent FROM snippets WHERE source='doc'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(cmd, "docker logs");
        assert!(intent.contains("Fetch the logs"));
        // 全文在旁表
        let id: i64 = conn.query_row("SELECT id FROM snippets WHERE source='doc'", [], |r| r.get(0)).unwrap();
        assert!(get_body(&conn, id).unwrap().contains("SYNOPSIS"));
        // 选项进参数树（--since 等已存在则跳过，但 follow 应为新增）
        assert!(db::count(&conn, "param_tree") > before);
        // 重复导入幂等
        let res2 = import_docs_dir(&conn, &dir).unwrap();
        assert_eq!(res2.snippets, 0);
        assert_eq!(res2.skipped, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
