//! 参数发散面板（设计 §4.3）：参数树分组 + 你的常用组合 + 相关命令。

use rusqlite::{params, Connection, ToSql};

use crate::db;
use crate::models::{ExplorePanel, ParamGroup, ParamItem, Pattern, Snippet};

/// 分组展示顺序（未知分类排最后，label 用原值）。
pub const CATEGORY_LABELS: &[(&str, &str)] = &[
    ("time", "时间过滤"),
    ("output", "输出控制"),
    ("filter", "内容过滤"),
    ("pipe", "管道过滤"),
    ("run", "运行控制"),
    ("other", "其他"), // v0.2 --help 生成的未归类参数
];

pub fn label_of(key: &str) -> String {
    CATEGORY_LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| (*v).to_string())
        .unwrap_or_else(|| key.to_string())
}

/// "docker" + "logs" → "docker logs"；tool 为空时退回命令首词。
pub fn base_name_of(sn: &Snippet) -> String {
    let tool = sn.tool.clone().unwrap_or_default();
    let command = sn.command.clone().unwrap_or_default();
    if !tool.is_empty() {
        if command.is_empty() {
            tool
        } else {
            format!("{tool} {command}")
        }
    } else {
        sn.cmd.split_whitespace().next().unwrap_or("").to_string()
    }
}

/// 组合使用量 +1（复制组合时回写，为权重排序积累数据）。
pub fn record_pattern(conn: &Connection, base_cmd: &str, suffix: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO patterns(base_cmd, suffix, used_count) VALUES (?1, ?2, 1)
         ON CONFLICT(base_cmd, suffix) DO UPDATE SET used_count = used_count + 1",
        params![base_cmd, suffix],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_explore(conn: &Connection, id: i64) -> Result<Option<ExplorePanel>, String> {
    let Some(base) = db::load_snippet(conn, id)? else {
        return Ok(None);
    };
    let tool = base.tool.clone().unwrap_or_default();
    let command = base.command.clone().unwrap_or_default();
    let base_name = base_name_of(&base);

    // 1) 参数树，按固定分类顺序分组
    let items: Vec<ParamItem> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, IFNULL(category,'') AS c, IFNULL(flag,'') AS f,
                        IFNULL(\"desc\",'') AS d, IFNULL(example, flag) AS ex
                 FROM param_tree
                 WHERE tool = ?1 AND IFNULL(command,'') = ?2
                 ORDER BY sort_order, id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![tool, command], |r| {
                Ok(ParamItem {
                    id: r.get("id")?,
                    category: r.get("c")?,
                    flag: r.get("f")?,
                    desc: r.get("d")?,
                    example: r.get::<_, Option<String>>("ex")?.unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };

    let mut groups: Vec<ParamGroup> = Vec::new();
    for (key, label) in CATEGORY_LABELS {
        let its: Vec<ParamItem> = items.iter().filter(|p| p.category == *key).cloned().collect();
        if !its.is_empty() {
            groups.push(ParamGroup {
                key: (*key).to_string(),
                label: (*label).to_string(),
                items: its,
            });
        }
    }
    // 未知分类兜底（比如 v0.2 --help 抓取可能产生新分类）
    let mut seen_unknown: Vec<String> = Vec::new();
    for p in &items {
        if !CATEGORY_LABELS.iter().any(|(k, _)| *k == p.category) && !seen_unknown.contains(&p.category) {
            seen_unknown.push(p.category.clone());
        }
    }
    for cat in seen_unknown {
        let its: Vec<ParamItem> = items.iter().filter(|p| p.category == cat).cloned().collect();
        groups.push(ParamGroup {
            label: label_of(&cat),
            key: cat,
            items: its,
        });
    }

    // 2) 你的常用组合
    let patterns: Vec<Pattern> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, base_cmd, suffix, used_count FROM patterns
                 WHERE base_cmd = ?1 ORDER BY used_count DESC, id LIMIT 6",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&base_name], |r| {
                Ok(Pattern {
                    id: r.get(0)?,
                    base_cmd: r.get(1)?,
                    suffix: r.get(2)?,
                    used_count: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };

    // 3) 相关命令：同工具优先，不足再全局补
    let mut related: Vec<Snippet> = if tool.is_empty() {
        Vec::new()
    } else {
        db::query_snippets(
            conn,
            "SELECT * FROM snippets
             WHERE id != ?1 AND (variant_of IS NULL OR variant_of != ?1) AND tool = ?2
             ORDER BY used_count DESC, id DESC LIMIT 8",
            &[&id, &tool as &dyn ToSql],
        )?
    };
    if related.len() < 4 {
        let mut exclude: Vec<i64> = related.iter().map(|s| s.id).collect();
        exclude.push(id);
        let ph = std::iter::repeat("?")
            .take(exclude.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT * FROM snippets WHERE id NOT IN ({ph}) \
             ORDER BY used_count DESC, id DESC LIMIT 8"
        );
        let refs: Vec<&dyn ToSql> = exclude.iter().map(|v| v as &dyn ToSql).collect();
        let extra = db::query_snippets(conn, &sql, &refs)?;
        related.extend(extra);
        related.truncate(8);
    }
    for r in related.iter_mut() {
        db::attach_tags(conn, r);
    }

    Ok(Some(ExplorePanel {
        base,
        base_name,
        groups,
        patterns,
        related,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewSnippet;

    #[test]
    fn explore_groups_and_patterns() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();

        let id = db::insert_snippet(
            &conn,
            NewSnippet {
                intent: "docker 日志",
                cmd: "docker logs -f web",
                project: None,
                tool: Some("docker"),
                command: Some("logs"),
                variant_of: None,
                source: "self",
                verified: true,
                used_count: 1,
                last_used: None,
                notes: None,
            },
        )
        .unwrap();

        let panel = get_explore(&conn, id).unwrap().expect("panel should exist");
        assert_eq!(panel.base_name, "docker logs");
        let keys: Vec<&str> = panel.groups.iter().map(|g| g.key.as_str()).collect();
        assert!(keys.contains(&"time"), "应有时间过滤组: {keys:?}");
        assert!(keys.contains(&"pipe"), "应有管道过滤组: {keys:?}");
        assert!(!panel.patterns.is_empty(), "种子常用组合应出现");
        assert!(!panel.related.is_empty());
    }

    #[test]
    fn pattern_upsert_accumulates() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        record_pattern(&conn, "git log", "| head -5").unwrap();
        record_pattern(&conn, "git log", "| head -5").unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT used_count FROM patterns WHERE base_cmd='git log' AND suffix='| head -5'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }
}
