//! tldr 页解析与目录导入：支持任意 tldr 格式仓库（官方英文库或社区中文翻译版）。
//!
//! 页面格式（tldr 标准）：
//!   # 命令名
//!   > 页面描述（可多行）
//!   - 示例说明（人话）
//!   `示例命令`

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::db;
use crate::models::ImportResult;
use crate::tagging;

pub struct ParsedPage {
    pub title: String,
    pub desc: String,
    /// (示例说明, 示例命令)
    pub examples: Vec<(String, String)>,
}

pub fn parse_page(content: &str) -> ParsedPage {
    let content = content.trim_start_matches('\u{feff}'); // 去 UTF-8 BOM
    let mut title = String::new();
    let mut desc: Vec<String> = Vec::new();
    let mut examples: Vec<(String, String)> = Vec::new();
    let mut pending: Option<String> = None;

    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("# ") && title.is_empty() {
            title = t[2..].trim().to_string();
        } else if t.starts_with('>') {
            desc.push(t.trim_start_matches('>').trim().to_string());
        } else if t.starts_with("- ") {
            pending = Some(t[2..].trim().to_string());
        } else if t.len() > 2 && t.starts_with('`') && t.ends_with('`') {
            if let Some(d) = pending.take() {
                // 跳过"更多信息"链接行
                if !d.starts_with("更多信息") && !d.starts_with("More information") {
                    examples.push((d, t[1..t.len() - 1].to_string()));
                }
            }
        }
    }

    ParsedPage {
        title,
        desc: desc.join(" "),
        examples,
    }
}

pub struct PageImport {
    pub pages: i64,
    pub added: i64,
    pub skipped: i64,
}

/// 解析并入库一页。重复（同 cmd + 同 intent 的 tldr 记录）自动跳过。
pub fn import_text(conn: &Connection, content: &str) -> PageImport {
    let page = parse_page(content);
    if page.title.is_empty() {
        return PageImport { pages: 0, added: 0, skipped: 0 };
    }
    let title_toks: Vec<&str> = page.title.split_whitespace().collect();
    let page_tool = title_toks.first().map(|s| (*s).to_string());
    let page_command = title_toks.get(1).map(|s| (*s).to_string());

    let mut added = 0i64;
    let mut skipped = 0i64;
    for (d, cmd) in &page.examples {
        let intent = if d.trim().is_empty() { page.title.clone() } else { d.clone() };
        let dup: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snippets WHERE source='tldr' AND cmd = ?1 AND intent = ?2",
                params![cmd, intent],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if dup > 0 {
            skipped += 1;
            continue;
        }
        let (ex_tool, ex_command) = tagging::derive_tool_command(cmd);
        let tool = page_tool.clone().or(ex_tool);
        let command = page_command.clone().or(ex_command);
        let tags = tagging::auto_tags(cmd, &intent);
        let notes = if page.desc.is_empty() { None } else { Some(page.desc.clone()) };
        let ok = db::insert_snippet(
            conn,
            db::NewSnippet {
                intent: &intent,
                cmd,
                project: None,
                tool: tool.as_deref(),
                command: command.as_deref(),
                variant_of: None,
                source: "tldr",
                verified: false,
                used_count: 0,
                last_used: None,
                notes: notes.as_deref(),
            },
        );
        match ok {
            Ok(id) => {
                let _ = db::set_tags(conn, id, &tags);
                added += 1;
            }
            Err(_) => skipped += 1,
        }
    }
    PageImport { pages: 1, added, skipped }
}

/// 展开 ~ 开头路径。
pub fn expand_path(p: &str) -> PathBuf {
    let p = p.trim().trim_matches('"');
    if let Some(rest) = p.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest.trim_start_matches('/').trim_start_matches('\\'));
        }
    }
    PathBuf::from(p)
}

/// 递归导入目录下所有 tldr 格式 .md（tldr 仓库结构 pages/<platform>/*.md）。
/// v0.2：整体包一个显式事务——万级示例导入从每条语句一次落盘变为一次提交。
pub fn import_dir(conn: &Connection, root: &Path) -> Result<ImportResult, String> {
    if !root.is_dir() {
        return Err(format!("目录不存在: {}", root.display()));
    }
    let mut files: Vec<PathBuf> = Vec::new();
    walk(root, &mut files, 0);
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let mut res = ImportResult { pages: 0, snippets: 0, skipped: 0 };
    for f in &files {
        let Ok(content) = std::fs::read_to_string(f) else { continue };
        let one = import_text(conn, &content);
        res.pages += one.pages;
        res.snippets += one.added;
        res.skipped += one.skipped;
    }
    if let Err(e) = conn.execute_batch("COMMIT") {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(format!("提交导入事务失败: {e}"));
    }
    Ok(res)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 || out.len() > 5000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if p.is_dir() {
            if name != "assets" && !name.starts_with('.') {
                walk(&p, out, depth + 1);
            }
        } else if name.ends_with(".md") && name != "index.md" && !name.starts_with("readme") {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tldr_page() {
        let md = "\u{feff}# docker logs\n\n> 查看容器日志。\n> 更多信息：<https://docs.docker.com>。\n\n- 查看容器日志：\n\n`docker logs {{容器}}`\n\n- 跟随最新输出：\n\n`docker logs -f --tail 100 {{容器}}`\n";
        let page = parse_page(md);
        assert_eq!(page.title, "docker logs");
        assert!(page.desc.contains("容器日志"));
        assert_eq!(page.examples.len(), 2);
        assert_eq!(page.examples[0].1, "docker logs {{容器}}");
    }

    #[test]
    fn import_and_dedup() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        let md = "# grep\n\n> 文本搜索。\n\n- 显示行号：\n\n`grep -n \"x\" f.txt`\n";
        let r1 = import_text(&conn, md);
        assert_eq!((r1.pages, r1.added), (1, 1));
        let r2 = import_text(&conn, md);
        assert_eq!(r2.added, 0, "重复导入应跳过");
        assert_eq!(r2.skipped, 1);
        let src: String = conn
            .query_row("SELECT source FROM snippets LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(src, "tldr");
    }

    #[test]
    fn expand_home() {
        let p = expand_path("~/foo");
        assert!(!p.to_string_lossy().contains('~'));
        assert!(p.to_string_lossy().ends_with("foo"));
    }
}
