//! 检索 + 权重排序（设计 §4.2）。
//!
//! 检索路径（v0.2 起按数据量自动分档）：
//! - 行数 ≤ `FTS_THRESHOLD`：LIKE 候选（中文两字词天然可命中）
//! - 行数 > `FTS_THRESHOLD` 且关键词全为 ASCII（≥3 字符）：FTS5 MATCH 候选（亚毫秒）
//! - 其余情况回退 LIKE——中文检索永远可用
//! 标签批量加载（v0.2：一次查询取回全部候选标签，消灭 N+1）；打分始终在 Rust 侧。

use std::collections::HashMap;

use chrono::{Local, NaiveDate};
use rusqlite::{Connection, ToSql};

use crate::db;
use crate::models::{Hit, Snippet};

/// 超过该行数且关键词适合 FTS 时，切换 FTS5 候选检索。
pub const FTS_THRESHOLD: i64 = 20_000;

/// 中文/短词回退 LIKE 时只扫的自有数据源——doc 层（批量手册）体量大且正文为英文，
/// 中文关键词扫它没有意义，永远排除（v1.1 优化包 ②）。
const OWN_SOURCES: &str = "('self','tldr','ai')";

/// 每个关键词在 7 个字段上做 LIKE 匹配（含标签关联子查询）。
/// 用匿名占位符 ?（按出现顺序绑定），多个关键词拼接时参数天然对齐。
const CAND_COND: &str = "(s.intent LIKE ? ESCAPE '\\' OR s.cmd LIKE ? ESCAPE '\\' \
     OR s.notes LIKE ? ESCAPE '\\' OR s.tool LIKE ? ESCAPE '\\' \
     OR s.command LIKE ? ESCAPE '\\' OR s.project LIKE ? ESCAPE '\\' \
     OR EXISTS (SELECT 1 FROM snippet_tags st JOIN tags t ON t.id = st.tag_id \
                WHERE st.snippet_id = s.id AND t.name LIKE ? ESCAPE '\\'))";

fn escape_like(kw: &str) -> String {
    kw.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn search(
    conn: &Connection,
    q: &str,
    project: Option<&str>,
    limit: usize,
) -> Result<Vec<Hit>, String> {
    search_with_opts(conn, q, project, limit, FTS_THRESHOLD)
}

/// `fts_threshold` 可注入（测试用）：候选行数超过它且关键词适合时走 FTS。
pub fn search_with_opts(
    conn: &Connection,
    q: &str,
    project: Option<&str>,
    limit: usize,
    fts_threshold: i64,
) -> Result<Vec<Hit>, String> {
    let keywords: Vec<String> = q
        .split_whitespace()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();

    let mut cands: Vec<Snippet> = if keywords.is_empty() {
        // 空查询：常用优先（近期使用的个人记录天然靠前）
        db::query_snippets(
            conn,
            "SELECT * FROM snippets ORDER BY used_count DESC, last_used DESC, id DESC LIMIT 200",
            &[],
        )?
    } else if fts_threshold < db::snippet_count(conn) && db::has_fts(conn) {
        // v1.1 优化包 ②：大库双路合并——
        //   FTS：ASCII ≥3 字符词查全库（含 doc 层，亚毫秒）
        //   LIKE：全部关键词只扫自有数据（self/tldr/ai，永远只有几 MB），
        //         补齐 FTS 覆盖不到的标签/项目/工具名字段与中文/短词
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<Snippet> = Vec::new();
        let ascii_kws = ascii_keywords(&keywords);
        if let Some(matchq) = fts_match_query(&ascii_kws) {
            for sn in fts_candidates(conn, &matchq)? {
                if seen.insert(sn.id) {
                    out.push(sn);
                }
            }
        }
        for sn in like_candidates(conn, &keywords, true)? {
            if seen.insert(sn.id) {
                out.push(sn);
            }
        }
        out
    } else {
        like_candidates(conn, &keywords, false)?
    };

    attach_tags_batch(conn, &mut cands);

    let mut hits: Vec<Hit> = cands
        .into_iter()
        .map(|sn| {
            let score = score(&sn, &keywords, project);
            Hit { sn, score }
        })
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.sn.used_count.cmp(&a.sn.used_count))
            .then(b.sn.last_used.cmp(&a.sn.last_used))
            .then(a.sn.id.cmp(&b.sn.id))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn like_candidates(conn: &Connection, keywords: &[String], own_only: bool) -> Result<Vec<Snippet>, String> {
    let conds: Vec<String> = keywords.iter().map(|_| CAND_COND.to_string()).collect();
    let mut params: Vec<String> = Vec::new();
    for kw in keywords {
        let like = format!("%{}%", escape_like(kw));
        for _ in 0..7 {
            params.push(like.clone());
        }
    }
    // own_only：大库路径下中文/短词只扫自有数据（self/tldr/ai）
    let scope = if own_only {
        format!(" AND s.source IN {OWN_SOURCES}")
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT s.* FROM snippets s WHERE {}{}",
        conds.join(" OR "),
        scope
    );
    let refs: Vec<&dyn ToSql> = params.iter().map(|s| s as &dyn ToSql).collect();
    db::query_snippets(conn, &sql, &refs)
}

fn ascii_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .filter(|k| k.len() >= 3 && k.is_ascii())
        .cloned()
        .collect()
}

/// FTS5 MATCH 查询串：关键词全部是 ASCII 且 ≥3 字符才适用（unicode61 分词切不出中文词）。
fn fts_match_query(keywords: &[String]) -> Option<String> {
    if keywords.is_empty()
        || !keywords.iter().all(|k| k.len() >= 3 && k.is_ascii())
    {
        return None;
    }
    Some(
        keywords
            .iter()
            .map(|k| format!("\"{}\"", k.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

fn fts_candidates(conn: &Connection, matchq: &str) -> Result<Vec<Snippet>, String> {
    db::query_snippets(
        conn,
        "SELECT s.* FROM snippets s JOIN snippets_fts ON s.id = snippets_fts.rowid \
         WHERE snippets_fts MATCH ?1",
        &[&matchq],
    )
}

/// 一次查询取回全部候选的标签（v0.2：替代逐条 attach 的 N+1）。
fn attach_tags_batch(conn: &Connection, sns: &mut [Snippet]) {
    if sns.is_empty() {
        return;
    }
    let ids: Vec<i64> = sns.iter().map(|s| s.id).collect();
    let ph = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT st.snippet_id, t.name FROM snippet_tags st JOIN tags t ON t.id = st.tag_id \
         WHERE st.snippet_id IN ({ph})"
    );
    let refs: Vec<&dyn ToSql> = ids.iter().map(|v| v as &dyn ToSql).collect();
    let Ok(mut stmt) = conn.prepare(&sql) else { return };
    let Ok(rows) = stmt.query_map(refs.as_slice(), |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    }) else {
        return;
    };
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    for (id, name) in rows.flatten() {
        map.entry(id).or_default().push(name);
    }
    for sn in sns.iter_mut() {
        if let Some(mut tags) = map.remove(&sn.id) {
            tags.sort();
            sn.tags = tags;
        }
    }
}

/// 设计 §4.2 权重 + 项目命中加成。
pub fn score(sn: &Snippet, keywords: &[String], project: Option<&str>) -> i64 {
    let mut s: i64 = 0;
    if sn.source == "self" {
        s += 50;
    }
    if let (Some(cur), Some(mine)) = (project, sn.project.as_deref()) {
        if !cur.is_empty() && !mine.is_empty() && mine.eq_ignore_ascii_case(cur) {
            s += 30;
        }
    }
    s += sn.used_count * 2;
    if let Some(lu) = sn.last_used.as_deref() {
        if let Ok(d) = NaiveDate::parse_from_str(lu, "%Y-%m-%d") {
            let diff = (Local::now().date_naive() - d).num_days();
            if (0..=7).contains(&diff) {
                s += 20;
            }
        }
    }
    let intent = sn.intent.to_lowercase();
    let cmd = sn.cmd.to_lowercase();
    let base = format!(
        "{} {}",
        sn.tool.clone().unwrap_or_default(),
        sn.command.clone().unwrap_or_default()
    )
    .to_lowercase();
    for kw in keywords {
        if sn.tags.iter().any(|t| t == kw) {
            s += 10;
        }
        if intent.contains(kw.as_str()) {
            s += 5;
        }
        if cmd.contains(kw.as_str()) || base.contains(kw.as_str()) {
            s += 2;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewSnippet;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        conn
    }

    fn add(conn: &Connection, intent: &str, cmd: &str, project: Option<&str>, source: &str, used: i64, last: Option<&str>) -> i64 {
        let (tool, command) = crate::tagging::derive_tool_command(cmd);
        let id = db::insert_snippet(
            conn,
            NewSnippet {
                intent,
                cmd,
                project,
                tool: tool.as_deref(),
                command: command.as_deref(),
                variant_of: None,
                source,
                verified: true,
                used_count: used,
                last_used: last,
                notes: None,
            },
        )
        .unwrap();
        let tags = crate::tagging::merge_tags(None, cmd, intent);
        db::set_tags(conn, id, &tags).unwrap();
        id
    }

    #[test]
    fn ranks_self_project_and_usage_first() {
        let conn = setup();
        let a = add(&conn, "docker logs 按时间过滤错误", "docker logs -f web --since 1h | grep ERROR", Some("shop-admin"), "self", 12, Some(&db::today()));
        let b = add(&conn, "grep 在日志里过滤错误", "grep -C 5 ERROR app.log", None, "tldr", 0, None);
        let c = add(&conn, "docker logs 末尾 100 行", "docker logs --tail 100 -t web", Some("other-proj"), "self", 1, None);

        let hits = search(&conn, "docker 日志", Some("shop-admin"), 20).unwrap();
        assert_eq!(hits[0].sn.id, a, "本项目+高频记录应排第一");
        assert!(hits.iter().any(|h| h.sn.id == b), "tldr 结果也应出现");
        assert!(hits.iter().any(|h| h.sn.id == c));
        // 权重可解释：a = 50(self)+30(项目)+24(次数)+20(7天内)+…
        assert!(hits[0].score >= 124);
        // v0.2 批量标签加载后，标签必须仍然完整带出
        assert!(!hits[0].sn.tags.is_empty(), "批量加载不能丢标签");
    }

    #[test]
    fn chinese_two_char_keyword_matches() {
        let conn = setup();
        let a = add(&conn, "gradle bootRun 指定端口启动", "./gradlew bootRun --args=--server.port=8081", Some("tools-help"), "self", 3, None);
        let hits = search(&conn, "端口", None, 20).unwrap();
        assert!(hits.iter().any(|h| h.sn.id == a), "两字中文关键词必须能命中（FTS5 做不到的事）");
    }

    #[test]
    fn empty_query_returns_most_used() {
        let conn = setup();
        add(&conn, "低频", "git status", None, "self", 1, None);
        let hot = add(&conn, "高频", "git log --oneline", None, "self", 9, None);
        let hits = search(&conn, "", None, 20).unwrap();
        assert_eq!(hits[0].sn.id, hot);
    }

    #[test]
    fn fts_path_agrees_with_like_for_ascii() {
        let conn = setup();
        assert!(db::has_fts(&conn), "bundled SQLite 应含 FTS5");
        add(&conn, "docker logs follow", "docker logs -f web", None, "self", 2, None);
        add(&conn, "docker ps list", "docker ps -a", None, "self", 1, None);
        add(&conn, "git log oneline", "git log --oneline -5", None, "self", 4, None);

        let like = search_with_opts(&conn, "docker logs", None, 20, i64::MAX).unwrap();
        let fts = search_with_opts(&conn, "docker logs", None, 20, 0).unwrap();
        let ids = |v: &[Hit]| v.iter().map(|h| h.sn.id).collect::<Vec<_>>();
        assert_eq!(ids(&like), ids(&fts), "大库路径应覆盖 LIKE 全量结果（自有数据）");
        assert!(!fts.is_empty());
    }

    #[test]
    fn chinese_keywords_skip_doc_layer_at_scale() {
        let conn = setup();
        let own = add(&conn, "查磁盘占用的中文记录", "du -sh .", Some("proj"), "self", 1, None);
        let doc = add(&conn, "doc page with chinese word 磁盘占用 in intent", "some-tool --flag", None, "doc", 0, None);

        // 小库（阈值极大）：LIKE 全量，doc 也能被中文词命中
        let small = search_with_opts(&conn, "磁盘占用", None, 20, i64::MAX).unwrap();
        assert!(small.iter().any(|h| h.sn.id == doc));

        // 大库（阈值 0 触发双路）：中文词只扫自有数据，doc 层不再命中——这正是优化点
        let big = search_with_opts(&conn, "磁盘占用", None, 20, 0).unwrap();
        assert!(big.iter().any(|h| h.sn.id == own), "自有数据中文命中保留");
        assert!(!big.iter().any(|h| h.sn.id == doc), "doc 层不被中文 LIKE 扫描");
    }

    #[test]
    fn fts_query_builder_rules() {
        assert_eq!(
            fts_match_query(&["docker".into(), "logs".into()]).unwrap(),
            "\"docker\" OR \"logs\""
        );
        // 中文 / 短词必须回退 LIKE
        assert!(fts_match_query(&["日志".into()]).is_none());
        assert!(fts_match_query(&["ab".into()]).is_none());
    }
}
