//! v0.2：官方 `--help` 抓取 → 参数树自动生成。
//!
//! 只在用户显式操作（面板按钮 / `snip gen-params`）时运行目标命令的 help：
//! - 工具名/子命令做字符白名单校验（拒绝路径分隔符等）
//! - 8 秒超时强杀；输出上限 256KB
//! - Windows 下 CREATE_NO_WINDOW，避免 GUI 里闪黑框
//! - git 特殊用 `-h`（`git --help` 会拉起 man 分页器）

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::{params, Connection};

const HELP_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_OUTPUT: usize = 256 * 1024;
const MAX_PARAMS: usize = 200;

#[derive(Debug)]
pub struct GeneratedParam {
    /// 展示用，如 "-f, --follow" 或 "--since"
    pub flag: String,
    /// 追加到组合里的示例形态，如 "--since <string>"
    pub example: String,
    pub desc: String,
    pub category: &'static str,
}

fn valid_token(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 64
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// 运行 `tool [sub] --help` 并合并 stdout/stderr。
pub fn capture_help(tool: &str, sub: Option<&str>) -> Result<String, String> {
    if !valid_token(tool) {
        return Err(format!("工具名不合法: {tool}"));
    }
    if let Some(s) = sub.filter(|s| !s.is_empty()) {
        if !valid_token(s) {
            return Err(format!("子命令名不合法: {s}"));
        }
    }
    let help_flag = if tool == "git" { "-h" } else { "--help" };

    let mut cmd = Command::new(tool);
    if let Some(s) = sub.filter(|s| !s.is_empty()) {
        cmd.arg(s);
    }
    cmd.arg(help_flag);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法运行 {tool}（不在 PATH？）: {e}"))?;

    // 读管道放线程；主线程轮询超时，必要时强杀
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut o) = stdout {
            let _ = o.read_to_string(&mut buf);
        }
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut buf);
        }
        let _ = tx.send(buf);
    });

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(format!("等待 {tool} 退出失败: {e}")),
        }
        if start.elapsed() > HELP_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{tool} {help_flag} 超过 8 秒未退出，已终止"));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut out = rx
        .recv_timeout(HELP_TIMEOUT * 2)
        .unwrap_or_default();
    if out.len() > MAX_OUTPUT {
        out.truncate(MAX_OUTPUT);
    }
    if out.trim().is_empty() {
        return Err(format!("{tool} {help_flag} 没有输出内容"));
    }
    Ok(out)
}

/// 噪声参数：--help/--version 本身没有发散价值。
fn is_noise(long_or_short: &str) -> bool {
    matches!(long_or_short, "--help" | "-h" | "--version" | "-v" | "--usage")
}

/// 从「参数段 + 说明」构造参数（供 --help 与 man 两种解析器复用）。
/// 参数段形如 "-f, --follow" / "--since string" / "-n <num>"；噪声返回 None。
pub(crate) fn param_from_spec(spec: &str, desc: String) -> Option<GeneratedParam> {
    let mut long: Option<String> = None;
    let mut short: Option<String> = None;
    let mut value_hint: Option<String> = None;
    for tok in spec.split_whitespace() {
        let tok = tok.trim_matches(',');
        if tok.starts_with("--") && tok.len() > 2 {
            long = Some(tok.to_string());
        } else if tok.starts_with('-') && tok.len() > 1 {
            short = Some(tok.to_string());
        } else if long.is_some() || short.is_some() {
            // flag 后面的占位符（string / <n> / N …）
            value_hint.get_or_insert_with(|| tok.to_string());
        }
    }
    let main = long.clone().or_else(|| short.clone())?;
    if is_noise(&main) {
        return None;
    }
    let flag = match (&long, &short) {
        (Some(l), Some(s)) => format!("{s}, {l}"),
        (Some(l), None) => l.clone(),
        (None, Some(s)) => s.clone(),
        (None, None) => return None,
    };
    let example = match &value_hint {
        Some(h) if h.starts_with('<') => format!("{main} {h}"),
        Some(h) => format!("{main} <{h}>"),
        None => main.clone(),
    };
    let category = infer_category(&main, &desc);
    Some(GeneratedParam { flag, example, desc, category })
}

/// 解析常见 help 格式（GNU / docker / kubectl / git -h 均为「缩进 + 参数段 + ≥2空格 + 说明」）。
pub fn parse_help(text: &str) -> Vec<GeneratedParam> {
    let mut out: Vec<GeneratedParam> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();

    for raw in text.lines() {
        if out.len() >= MAX_PARAMS {
            break;
        }
        let trimmed = raw.trim();
        if !trimmed.starts_with('-') || trimmed.len() < 2 {
            continue;
        }
        // 参数行通常有缩进（docker 2/6、git 4、Windows curl 1），排除顶格正文；
        // 顶格但带 ≥2 空格分隔说明的行也放行（busybox 风格）
        let indent = raw.len() - raw.trim_start().len();
        if indent < 1 && !trimmed.contains("  ") {
            continue;
        }
        let mut segs = trimmed.split("  ").filter(|s| !s.trim().is_empty());
        let Some(spec) = segs.next().map(str::trim) else { continue };
        let desc: String = segs
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .chars()
            .take(120)
            .collect();
        if let Some(p) = param_from_spec(spec, desc) {
            let key = p.flag.rsplit(", ").next().unwrap_or(&p.flag).to_string();
            if seen.insert(key) {
                out.push(p);
            }
        }
    }
    out
}

fn infer_category(flag: &str, desc: &str) -> &'static str {
    let hay = format!("{flag} {desc}").to_lowercase();
    let has = |kws: &[&str]| kws.iter().any(|k| hay.contains(k));
    if has(&["since", "until", "time", "date", "duration", "timeout", "时间"]) {
        "time"
    } else if has(&[
        "tail", "head", "verbose", "quiet", "silent", "output", "format", "color",
        "details", "detail", "debug", "dry", "输出", "详细",
    ]) {
        "output"
    } else if has(&[
        "filter", "match", "select", "exclude", "include", "grep", "selector",
        "label", "name", "search", "过滤",
    ]) {
        "filter"
    } else if has(&["follow", "watch", "wait", "detach", "interval", "exec", "run"]) {
        "run"
    } else {
        "other"
    }
}

fn flag_exists(conn: &Connection, tool: &str, sub: &str, key_flag: &str) -> bool {
    let like = format!("%{key_flag}%");
    conn.query_row(
        "SELECT COUNT(*) FROM param_tree WHERE tool = ?1 AND IFNULL(command,'') = ?2 AND flag LIKE ?3",
        params![tool, sub, like],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// 抓取 + 解析 + 入库。返回 (解析出的参数数, 实际新增数)；与已有参数树按长参数名去重。
pub fn generate_into(conn: &Connection, tool: &str, sub: Option<&str>) -> Result<(usize, usize), String> {
    let sub_norm = sub.unwrap_or("").trim();
    let text = capture_help(tool, Some(sub_norm).filter(|s| !s.is_empty()))?;
    let params = parse_help(&text);
    if params.is_empty() {
        return Err("未能从 --help 输出中解析出参数（该工具的 help 格式可能不标准）".into());
    }
    let parsed = params.len();
    let added = insert_params(conn, tool, sub_norm, &params)?;
    Ok((parsed, added))
}

/// 参数入树（供 --help 与 man 导入复用）：按长参数名与已有条目去重，排序接在已有之后。
pub(crate) fn insert_params(
    conn: &Connection,
    tool: &str,
    sub: &str,
    params: &[GeneratedParam],
) -> Result<usize, String> {
    let mut next_order: i64 = conn
        .query_row(
            "SELECT IFNULL(MAX(sort_order), 0) FROM param_tree WHERE tool = ?1 AND IFNULL(command,'') = ?2",
            params![tool, sub],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut added = 0usize;
    for p in params {
        let key = p.flag.rsplit(", ").next().unwrap_or(&p.flag).to_string();
        if flag_exists(conn, tool, sub, &key) {
            continue;
        }
        next_order += 1;
        conn.execute(
            "INSERT INTO param_tree(tool, command, category, flag, \"desc\", example, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![tool, sub, p.category, p.flag, p.desc, p.example, next_order],
        )
        .map_err(|e| e.to_string())?;
        added += 1;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const DOCKER_LOGS_HELP: &str = "\
Usage:  docker logs [OPTIONS] NAME

Fetch the logs of a container

Options:
      --details        Show extra details provided to logs
  -f, --follow         Follow log output
      --since string   Show logs since timestamp (e.g. 42m)
  -n, --tail string    Number of lines to show from the end of the logs
  -t, --timestamps     Show timestamps
      --until string   Show logs before a timestamp
      --help           Print help
";

    #[test]
    fn parse_docker_style_help() {
        let ps = parse_help(DOCKER_LOGS_HELP);
        let by_flag = |f: &str| ps.iter().find(|p| p.flag == f);

        assert_eq!(ps.len(), 6, "--help 应被过滤：{ps:?}");
        let follow = by_flag("-f, --follow").expect("短+长参数应合并展示");
        assert_eq!(follow.desc, "Follow log output");
        assert_eq!(follow.example, "--follow");

        let since = by_flag("--since").expect("长参数独立行");
        assert_eq!(since.example, "--since <string>", "占位符应拼进示例");
        assert_eq!(since.category, "time");

        let tail = by_flag("-n, --tail").expect("--tail");
        assert_eq!(tail.category, "output");
        assert!(by_flag("-t, --timestamps").is_some());
    }

    #[test]
    fn parse_git_style_help() {
        let git = "usage: git log [<options>]\n\n    -p, --patch           generate patch\n    -n <num>              limit to <num> entries\n    --oneline             shorthand\n";
        let ps = parse_help(git);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].flag, "-p, --patch");
        assert_eq!(ps[1].flag, "-n");
        assert_eq!(ps[1].example, "-n <num>", "尖括号占位符原样保留");
    }

    #[test]
    fn generate_dedupes_against_existing() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap(); // 已有手写 docker logs 参数树

        // 机器上若装了 docker 则真实走一遍（新增数不应超过解析数）；
        // 没装则返回 Err，这里只验证不 panic。解析逻辑由上面两个单测覆盖。
        if let Ok((parsed, added)) = generate_into(&conn, "docker", Some("logs")) {
            assert!(added <= parsed);
        }
    }

    #[test]
    fn rejects_bad_tool_names() {
        assert!(capture_help("../evil", None).is_err());
        assert!(capture_help("foo/bar", None).is_err());
        assert!(capture_help("", None).is_err());
    }
}
