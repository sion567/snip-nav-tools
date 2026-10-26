//! 首发种子：参数树 / 常用组合 / 示例个人记录 / 内置中文 tldr 页。
//! 全部只在对应表为空时写入，用户数据不会被覆盖。

use rusqlite::{params, Connection};

use crate::db::{self, NewSnippet};
use crate::importer;
use crate::tagging;

// (tool, command, category, flag, desc, example)
const PARAM_SEEDS: &[(&str, &str, &str, &str, &str, &str)] = &[
    // docker logs —— 手写参数树（设计稿 Day 4 示例）
    ("docker", "logs", "time", "--since <时间点>", "从指定时间开始", "--since 2026-10-25T00:00:00"),
    ("docker", "logs", "time", "--since 1h", "最近 1 小时", "--since 1h"),
    ("docker", "logs", "time", "--until <时间点>", "到指定时间结束", "--until 2026-10-25T12:00:00"),
    ("docker", "logs", "output", "--tail <N>", "只看最后 N 行", "--tail 100"),
    ("docker", "logs", "output", "-t, --timestamps", "显示时间戳", "-t"),
    ("docker", "logs", "output", "--details", "显示额外详情", "--details"),
    ("docker", "logs", "pipe", "| grep \"ERROR\"", "只留错误日志", "| grep \"ERROR\""),
    ("docker", "logs", "pipe", "| grep -C 5 \"ERROR\"", "带上下文 5 行", "| grep -C 5 \"ERROR\""),
    ("docker", "logs", "pipe", "| awk '{print $1}'", "只取第一列（时间）", "| awk '{print $1}'"),
    ("docker", "logs", "pipe", "| jq '.level'", "JSON 日志过滤", "| jq '.level'"),
    // docker ps
    ("docker", "ps", "output", "-a", "显示已停止的容器", "-a"),
    ("docker", "ps", "output", "-q", "只输出容器 ID", "-q"),
    ("docker", "ps", "filter", "--filter name=<关键字>", "按名称过滤", "--filter name=web"),
    ("docker", "ps", "output", "--format <模板>", "自定义输出格式", "--format '{{.Names}} {{.Status}}'"),
    // grep
    ("grep", "", "filter", "-i", "忽略大小写", "-i"),
    ("grep", "", "filter", "-n", "显示行号", "-n"),
    ("grep", "", "filter", "-r", "递归搜索目录", "-r"),
    ("grep", "", "filter", "-v", "反选（不含关键字）", "-v"),
    ("grep", "", "output", "-C <N>", "显示上下文 N 行", "-C 5"),
    ("grep", "", "output", "--color", "高亮匹配", "--color=auto"),
    // git log
    ("git", "log", "output", "--oneline", "单行显示", "--oneline"),
    ("git", "log", "output", "-n <N>", "最近 N 条", "-n 5"),
    ("git", "log", "filter", "--grep=<关键字>", "按提交信息过滤", "--grep=登录"),
    ("git", "log", "filter", "--author=<名字>", "按作者过滤", "--author=zhang"),
    ("git", "log", "time", "--since=<日期>", "从某日期开始", "--since=2026-10-01"),
    // gradle bootRun
    ("gradle", "bootRun", "run", "--args=<参数>", "传运行参数（如端口）", "--args='--server.port=8081'"),
    ("gradle", "bootRun", "output", "--info", "详细日志", "--info"),
    ("gradle", "bootRun", "run", "-x test", "跳过测试", "-x test"),
    // kubectl logs
    ("kubectl", "logs", "output", "-f", "实时跟随", "-f"),
    ("kubectl", "logs", "output", "--tail=<N>", "尾部 N 行", "--tail=100"),
    ("kubectl", "logs", "filter", "-n <namespace>", "指定命名空间", "-n prod"),
    ("kubectl", "logs", "filter", "-c <容器>", "指定容器", "-c app"),
];

// (base_cmd, suffix, used_count)
const PATTERN_SEEDS: &[(&str, &str, i64)] = &[
    ("docker logs", "| grep \"ERROR\"", 12),
    ("docker logs", "| grep -C 5 \"ERROR\"", 5),
    ("docker logs", "--tail 100 -t", 5),
    ("kubectl logs", "--tail=100 -f", 3),
];

// (intent, cmd, project, tags, used_count, days_ago, notes)
type SampleSeed = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    i64,
    i64,
    &'static str,
);
const SAMPLES: &[SampleSeed] = &[
    ("docker logs 按时间+级别过滤", "docker logs -f mycontainer --since 2026-10-25T00:00:00 | grep ERROR", "shop-admin", &["docker", "log", "filter"], 12, 0, "示例记录：可随时删除"),
    ("docker logs 只看末尾 100 行", "docker logs --tail 100 -t mycontainer", "shop-admin", &["docker", "log"], 5, 9, "示例记录：可随时删除"),
    ("gradle bootRun 指定端口启动", "./gradlew bootRun --args=\"--server.port=8081\"", "tools-help", &["gradle", "run", "network"], 8, 3, "示例记录：可随时删除"),
    ("git log 找包含关键字的提交", "git log --oneline --grep=\"登录\"", "shop-admin", &["git", "filter"], 6, 12, "示例记录：可随时删除"),
    ("kubectl 看 pod 日志尾部", "kubectl logs -f my-pod -n prod --tail=100", "tools-help", &["k8s", "log"], 3, 5, "示例记录：可随时删除"),
    ("查端口占用（Windows）", "netstat -ano | findstr :8080", "tools-help", &["windows", "network"], 9, 1, "示例记录：可随时删除"),
];

const BUILTIN_TLDR: &[&str] = &[
    include_str!("../seeds/tldr_zh/grep.md"),
    include_str!("../seeds/tldr_zh/curl.md"),
    include_str!("../seeds/tldr_zh/tar.md"),
    include_str!("../seeds/tldr_zh/jq.md"),
    include_str!("../seeds/tldr_zh/docker-logs.md"),
];

pub fn seed_all(conn: &Connection) -> Result<(), String> {
    // v1.2 P0-4：LIMIT 1 存在性判断，替代 COUNT(*) 全表计数
    if db::is_empty(conn, "param_tree") {
        for (i, row) in PARAM_SEEDS.iter().enumerate() {
            let (tool, command, category, flag, desc, example) = *row;
            conn.execute(
                "INSERT INTO param_tree(tool, command, category, flag, \"desc\", example, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![tool, command, category, flag, desc, example, (i as i64) + 1],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    if db::is_empty(conn, "patterns") {
        for &(base, suffix, used) in PATTERN_SEEDS {
            conn.execute(
                "INSERT INTO patterns(base_cmd, suffix, used_count) VALUES (?1, ?2, ?3)",
                params![base, suffix, used],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    if db::is_empty(conn, "snippets") {
        for s in SAMPLES {
            let (intent, cmd, project, tags, used, ago, notes) = *s;
            let (tool, command) = tagging::derive_tool_command(cmd);
            let id = db::insert_snippet(
                conn,
                NewSnippet {
                    intent,
                    cmd,
                    project: Some(project),
                    tool: tool.as_deref(),
                    command: command.as_deref(),
                    variant_of: None,
                    source: "self",
                    verified: true,
                    used_count: used,
                    last_used: Some(&db::days_ago(ago)),
                    notes: Some(notes),
                },
            )?;
            let mut all: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
            all.extend(tagging::auto_tags(cmd, intent));
            db::set_tags(conn, id, &all)?;
        }
        for content in BUILTIN_TLDR {
            importer::import_text(conn, content);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_populate_all_layers() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        seed_all(&conn).unwrap();
        seed_all(&conn).unwrap(); // 幂等：重复执行不翻倍
        assert!(db::count(&conn, "param_tree") > 20);
        assert!(db::count(&conn, "patterns") >= 4);
        let self_n: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets WHERE source='self'", [], |r| r.get(0))
            .unwrap();
        let tldr_n: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets WHERE source='tldr'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(self_n, 6, "示例个人记录应为 6 条");
        assert!(tldr_n > 10, "内置 tldr 页应产生 10+ 条速查");
    }
}
