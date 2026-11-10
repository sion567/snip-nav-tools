//! 参数发散面板（设计 §4.3）：参数树分组 + 你的常用组合 + 相关命令。

use rusqlite::{params, Connection, ToSql};

use crate::db;
use crate::models::{CmdPart, ExplainSeg, ExplorePanel, ParamGroup, ParamItem, Pattern, Snippet};

/// 分组展示顺序（未知分类排最后，label 用原值）。
pub const CATEGORY_LABELS: &[(&str, &str)] = &[
    ("time", "时间过滤"),
    ("output", "输出控制"),
    ("filter", "内容过滤"),
    ("pipe", "管道过滤"),
    ("run", "运行控制"),
    ("build", "构建/依赖"),
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

// ---------- v1.7：命令解释（explainshell 式） ----------

use std::collections::HashMap;

/// 加载某工具的 flag → 描述映射。
/// `command` 为 Some 时只收该子命令的参数树，查不到再回落到工具级（command=''）；
/// 不跨子命令取条目（docker ps 的 -a 不该解释 docker logs 的 -a）。
/// flag 字段形如 "--tail <N>" / "-t, --timestamps" / "--grep=<关键字>"，
/// 按逗号/空白拆出以 - 开头的候选名。
fn flag_desc_map(conn: &Connection, tool: &str, command: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if tool.is_empty() {
        return map;
    }
    let Ok(mut stmt) =
        conn.prepare("SELECT IFNULL(command,'') AS c, IFNULL(flag,'') AS f, IFNULL(\"desc\",'') AS d FROM param_tree WHERE tool = ?1")
    else {
        return map;
    };
    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![tool], |r| {
            Ok((r.get("c")?, r.get("f")?, r.get("d")?))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for (c, f, d) in rows {
        // 子命令级：仅当与目标子命令一致；工具级：command 为空字符串
        let take = match command {
            Some(k) => c == k || c.is_empty(),
            None => c.is_empty(),
        };
        if !take {
            continue;
        }
        for cand in f.split([',', ' ', '\t']) {
            let cand = cand.trim();
            // 剥掉 =<值> 形式（--tail=<N> → --tail），候选名只留旗标本身
            let cand = cand.split('=').next().unwrap_or(cand).trim();
            if cand.starts_with('-') && cand.len() >= 2 {
                // 子命令专属描述优先于工具级：已有条目时仅子命令级行才覆盖
                map.entry(cand.to_string())
                    .and_modify(|v| {
                        if !c.is_empty() {
                            *v = d.clone();
                        }
                    })
                    .or_insert_with(|| d.clone());
            }
        }
    }
    map
}

/// 解释单个 token：--flag=value 查 --flag；合并短旗标 -ft 逐字符查并拼合描述；
/// 其余片段（工具名、参数值）desc 为 None。
fn explain_token(tok: &str, map: &HashMap<String, String>) -> CmdPart {
    if let Some(rest) = tok.strip_prefix("--") {
        let name = rest.split('=').next().unwrap_or(rest);
        let desc = map.get(&format!("--{name}")).cloned();
        return CmdPart { text: tok.to_string(), desc };
    }
    if let Some(rest) = tok.strip_prefix('-') {
        if !rest.is_empty() && !rest.starts_with('-') && rest.len() > 1 {
            let mut descs = Vec::new();
            for c in rest.chars() {
                let key = format!("-{c}");
                if let Some(d) = map.get(&key) {
                    descs.push(format!("{key}：{d}"));
                }
            }
            let desc = if descs.is_empty() { None } else { Some(descs.join("；")) };
            return CmdPart { text: tok.to_string(), desc };
        }
        return CmdPart {
            desc: map.get(tok).cloned(),
            text: tok.to_string(),
        };
    }
    CmdPart { text: tok.to_string(), desc: None }
}

/// 把命令按 | 分段逐段解释；每段工具取首 token（./gradlew 归一为 gradle），
/// 第二个 token 非参数时视为子命令用于精准匹配参数树。
pub fn build_explain(conn: &Connection, cmd: &str) -> Vec<ExplainSeg> {
    let mut segs = Vec::new();
    for seg in cmd.split('|') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let tool = toks
            .first()
            .map(|t| match *t {
                "./gradlew" | "gradlew" => "gradle".to_string(),
                t => t.to_lowercase(),
            })
            .unwrap_or_default();
        let subcmd = toks
            .get(1)
            .filter(|t| !t.starts_with('-'))
            .map(|t| t.to_string());
        let map = flag_desc_map(conn, &tool, subcmd.as_deref());
        let parts = toks.iter().map(|t| explain_token(t, &map)).collect();
        segs.push(ExplainSeg { tool, parts });
    }
    segs
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

    // 4) v1.7 命令逐段解释
    let explain = build_explain(conn, &base.cmd);

    Ok(Some(ExplorePanel {
        base,
        base_name,
        groups,
        patterns,
        related,
        explain,
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
    fn explain_annotates_flags_and_pipes() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();

        // kubectl logs -fn --tail=100（种子有 -f/-n/--tail 描述）| grep -in error
        let segs = build_explain(&conn, "kubectl logs -fn my-pod --tail=100 | grep -in error");
        assert_eq!(segs.len(), 2, "按管道分两段");
        assert_eq!(segs[0].tool, "kubectl");
        assert_eq!(segs[1].tool, "grep");

        // kubectl 段：-fn 逐字符拆出 -f/-n 的描述合并；--tail=100 命中 --tail
        let fn_tok = segs[0].parts.iter().find(|p| p.text == "-fn").expect("应有 -fn 片段");
        let d = fn_tok.desc.as_deref().unwrap_or("");
        assert!(d.contains("-f") && d.contains("-n"), "合并短旗标描述应含两者: {d}");
        let tail = segs[0].parts.iter().find(|p| p.text == "--tail=100").expect("应有 --tail=100");
        assert_eq!(tail.desc.as_deref(), Some("尾部 N 行"));
        // 工具名/普通参数不带描述
        let pod = segs[0].parts.iter().find(|p| p.text == "my-pod").expect("应有 my-pod");
        assert_eq!(pod.desc, None);

        // grep 段：-in 是合并短旗标（种子有 -i/-n 描述）
        let in_tok = segs[1].parts.iter().find(|p| p.text == "-in").expect("应有 -in");
        let d2 = in_tok.desc.as_deref().unwrap_or("");
        assert!(d2.contains("忽略大小写") && d2.contains("显示行号"), "grep 段应命中种子描述: {d2}");
    }

    #[test]
    fn explain_no_cross_subcommand_contamination() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();

        // docker ps -a：-a 只在 docker ps 的参数树里有，解释 docker ps 时命中，
        // 解释 docker logs 时不应串到（logs 的参数树没有 -a）
        let segs = build_explain(&conn, "docker ps -a");
        let a = segs[0].parts.iter().find(|p| p.text == "-a").unwrap();
        assert_eq!(a.desc.as_deref(), Some("显示已停止的容器"));

        let segs2 = build_explain(&conn, "docker logs -a web");
        let a2 = segs2[0].parts.iter().find(|p| p.text == "-a").unwrap();
        assert_eq!(a2.desc, None, "logs 参数树没有 -a，不应借用 ps 的描述");
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
