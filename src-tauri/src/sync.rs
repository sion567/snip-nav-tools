//! v1.10：Git 仓库同步（pet Gist 式，团队长期协作）。
//!
//! `snip sync <仓库地址>`：clone/pull → 把远端 snippets/*.md 合并入库 →
//! 全量导出个人记录到 snippets/ → commit → push。
//!
//! - 每条记录一个 `<cmd哈希>.md`（frontmatter 存元数据，正文即命令），
//!   git diff 一行一记录，天然 review 友好
//! - 合并语义与 share.rs 一致：同命令取较大 used_count、置顶取或、并标签
//! - 已知限制：删除不传播（本地删掉的记录会在下次 pull 时被远端带回来），
//!   这是无冲突追踪的极简同步的取舍
//! - 鉴权交给用户本机的 git 凭据（SSH / credential manager），本工具不碰密钥
//! - commit 作者固定为 SnipNav（-c 覆盖），不依赖用户 git 配置

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::{params, Connection};

use crate::db::{self, NewSnippet};
use crate::tagging;

pub struct SyncStat {
    pub added: i64,
    pub merged: i64,
    pub exported: i64,
    pub pushed: bool,
}

// ---------- git 调用 ----------

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(args);
    let out = cmd
        .output()
        .map_err(|_| "未找到 git 命令（请先安装 Git 并加入 PATH）".to_string())?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // git commit 的 "nothing to commit" 等说明打在 stdout，一并带上便于排障
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("git {} 失败: {}", args.join(" "), detail));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// 克隆（首次）或复用已有克隆，返回仓库目录。
fn ensure_repo(url: &str) -> Result<PathBuf, String> {
    let dir = db::data_dir()?.join("sync");
    if dir.join(".git").exists() {
        return Ok(dir);
    }
    // 清掉可能存在的半残目录
    let _ = std::fs::remove_dir_all(&dir);
    let url = url.trim();
    git(None, &["clone", url, dir.to_str().ok_or("路径非法")?])
        .map_err(|e| format!("克隆失败（检查地址与凭据）: {e}"))?;
    Ok(dir)
}

// ---------- 记录 <-> markdown/frontmatter ----------

/// 命令内容哈希（FNV-1a）：文件名稳定且天然按命令去重。
fn cmd_hash(cmd: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in cmd.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn escape_field(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}

fn export_record(id: i64, intent: &str, cmd: &str, project: Option<&str>, tags: &[String], used: i64, pinned: bool) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("intent: {}\n", escape_field(intent)));
    if let Some(p) = project.filter(|p| !p.is_empty()) {
        out.push_str(&format!("project: {}\n", escape_field(p)));
    }
    if !tags.is_empty() {
        out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    }
    out.push_str(&format!("usedCount: {used}\n"));
    if pinned {
        out.push_str("pinned: true\n");
    }
    out.push_str("---\n");
    out.push_str(cmd);
    out.push('\n');
    let _ = id;
    out
}

struct RemoteRecord {
    intent: String,
    project: Option<String>,
    tags: Vec<String>,
    used: i64,
    pinned: bool,
    cmd: String,
}

/// 解析 frontmatter 文件；格式不对的文件返回 None（跳过）。
/// 行尾归一为 LF——Windows 上 git clone 默认 autocrlf 检出 CRLF，
/// 不归一会让所有文件解析失败（首次克隆后同步拿不到远端记录的元凶）。
fn parse_record(text: &str) -> Option<RemoteRecord> {
    let text = text.trim_start().replace("\r\n", "\n");
    let rest = text.strip_prefix("---\n")?;
    let (front, body) = rest.split_once("\n---")?;
    let mut intent = String::new();
    let mut project: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut used = 0i64;
    let mut pinned = false;
    for line in front.lines() {
        let Some((key, value)) = line.split_once(": ") else { continue };
        match key.trim() {
            "intent" => intent = value.trim().to_string(),
            "project" => project = Some(value.trim().to_string()),
            "tags" => {
                let v = value.trim().trim_start_matches('[').trim_end_matches(']');
                tags = v
                    .split(',')
                    .map(|t| t.trim().to_lowercase())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            "usedCount" => used = value.trim().parse().unwrap_or(0),
            "pinned" => pinned = value.trim() == "true",
            _ => {}
        }
    }
    let cmd = body.trim().to_string();
    if cmd.is_empty() {
        return None;
    }
    if intent.is_empty() {
        intent = cmd.lines().next().unwrap_or(&cmd).to_string();
    }
    Some(RemoteRecord { intent, project, tags, used, pinned, cmd })
}

// ---------- 拉取合并 / 导出 ----------

/// 远端 snippets/*.md 合并入库（同命令：取较大 used_count、置顶取或、并标签）。
fn import_remote(conn: &Connection, repo: &Path) -> Result<(i64, i64), String> {
    let dir = repo.join("snippets");
    let mut added = 0i64;
    let mut merged = 0i64;
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok((0, 0)); // 仓库还没有 snippets 目录 = 全新仓库
    };
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    for f in files {
        if f.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let Some(rec) = parse_record(&text) else { continue };
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM snippets WHERE cmd = ?1 AND source = 'self' LIMIT 1",
                params![rec.cmd],
                |r| r.get(0),
            )
            .ok();
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE snippets SET used_count = MAX(used_count, ?2), pinned = MAX(pinned, ?3) WHERE id = ?1",
                    params![id, rec.used, rec.pinned as i64],
                )
                .map_err(|e| e.to_string())?;
                let mut tags = db::snippet_tags(conn, id);
                for t in &rec.tags {
                    if !tags.contains(t) {
                        tags.push(t.clone());
                    }
                }
                db::set_tags(conn, id, &tags)?;
                merged += 1;
            }
            None => {
                let (tool, command) = tagging::derive_tool_command(&rec.cmd);
                let merged_tags = tagging::merge_tags(Some(&rec.tags), &rec.cmd, &rec.intent);
                let Ok(id) = db::insert_snippet(
                    conn,
                    NewSnippet {
                        intent: &rec.intent,
                        cmd: &rec.cmd,
                        project: rec.project.as_deref(),
                        tool: tool.as_deref(),
                        command: command.as_deref(),
                        variant_of: None,
                        source: "self",
                        verified: false,
                        used_count: rec.used,
                        last_used: None,
                        notes: Some("Git 同步引入"),
                    },
                ) else {
                    continue;
                };
                let _ = db::set_tags(conn, id, &merged_tags);
                added += 1;
            }
        }
    }
    Ok((added, merged))
}

/// 全量导出个人记录（source='self'）到 snippets/*.md（先清空目录防陈旧文件）。
fn export_all(conn: &Connection, repo: &Path) -> Result<i64, String> {
    let dir = repo.join("snippets");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("清理导出目录失败: {e}"))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败: {e}"))?;
    // 强制各端检出为 LF：Windows 默认 autocrlf 会把克隆出的文件变成 CRLF
    let _ = std::fs::write(
        repo.join(".gitattributes"),
        "*.md text eol=lf\n",
    );
    let mut stmt = conn
        .prepare(
            "SELECT id, intent, IFNULL(cmd,''), project, used_count, pinned
             FROM snippets WHERE source = 'self' ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String, String, Option<String>, i64, bool)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, i64>(5)? != 0))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    let mut n = 0i64;
    for (id, intent, cmd, project, used, pinned) in rows {
        if cmd.trim().is_empty() {
            continue;
        }
        let tags = db::snippet_tags(conn, id);
        let text = export_record(id, &intent, &cmd, project.as_deref(), &tags, used, pinned);
        let file = dir.join(format!("{}.md", cmd_hash(&cmd)));
        std::fs::write(&file, text).map_err(|e| format!("写入 {} 失败: {e}", file.display()))?;
        n += 1;
    }
    Ok(n)
}

// ---------- 主流程 ----------

/// 同步一个仓库地址：clone/pull → 合并 → 导出 → commit → push。
pub fn sync_repo(conn: &Connection, url: &str) -> Result<SyncStat, String> {
    let repo = ensure_repo(url)?;
    // pull：全新空仓库没有上游，失败不致命（磁盘上已有的照常合并）
    if let Err(e) = git(Some(&repo), &["pull", "--ff-only"]) {
        eprintln!("sync: pull 跳过（{e}）");
    }
    let (added, merged) = import_remote(conn, &repo)?;
    let exported = export_all(conn, &repo)?;
    let mut pushed = false;
    // add 之后用 diff --cached 判断：autocrlf 下工作区可能显示"幽灵修改"
    //（行尾归一后与远端完全一致），此时不该产生空提交
    git(Some(&repo), &["add", "snippets", ".gitattributes"])?;
    let staged = git(Some(&repo), &["diff", "--cached", "--name-only"]).unwrap_or_default();
    if !staged.trim().is_empty() {
        git(
            Some(&repo),
            &[
                "-c",
                "user.name=SnipNav",
                "-c",
                "user.email=snipnav@local",
                "commit",
                "-m",
                &format!("snip sync: {} records", exported),
            ],
        )?;
        git(Some(&repo), &["push", "-u", "origin", "HEAD"])
            .map_err(|e| format!("提交完成但推送失败（检查远端写权限）: {e}"))?;
        pushed = true;
    }
    Ok(SyncStat { added, merged, exported, pushed })
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
    fn record_md_roundtrip() {
        let md = export_record(
            1,
            "docker 日志 过滤",
            "docker logs -f web | grep ERROR",
            Some("shop-admin"),
            &["docker".to_string(), "log".to_string()],
            12,
            true,
        );
        let rec = parse_record(&md).expect("应可解析");
        assert_eq!(rec.intent, "docker 日志 过滤");
        assert_eq!(rec.cmd, "docker logs -f web | grep ERROR");
        assert_eq!(rec.project.as_deref(), Some("shop-admin"));
        assert_eq!(rec.tags, vec!["docker", "log"]);
        assert_eq!(rec.used, 12);
        assert!(rec.pinned);
    }

    #[test]
    fn record_md_fields_optional_and_multiline_cmd() {
        let md = export_record(2, "打包前端", "pnpm build\npnpm test", None, &[], 0, false);
        let rec = parse_record(&md).expect("应可解析");
        assert_eq!(rec.project, None, "无 project 不应出现该行");
        assert_eq!(rec.tags.len(), 0);
        assert!(!rec.pinned);
        assert_eq!(rec.cmd, "pnpm build\npnpm test", "多行命令走正文");
        // intent 里的冒号不破坏解析
        let md2 = export_record(3, "格式化: JSON 输出", "jq .", None, &[], 1, false);
        assert_eq!(parse_record(&md2).unwrap().intent, "格式化: JSON 输出");
    }

    #[test]
    fn parse_rejects_junk() {
        assert!(parse_record("不是 frontmatter 的普通文件").is_none());
        assert!(parse_record("---\nintent: 只有头没有正文\n---\n").is_none(), "空命令拒收");
    }

    #[test]
    fn parse_tolerates_crlf() {
        // Windows git clone 默认 autocrlf 检出 CRLF——解析必须容忍
        let md = "---\r\nintent: 查容器\r\nusedCount: 2\r\n---\r\ndocker ps\r\n";
        let rec = parse_record(md).expect("CRLF 应可解析");
        assert_eq!(rec.intent, "查容器");
        assert_eq!(rec.cmd, "docker ps");
        assert_eq!(rec.used, 2);
    }

    #[test]
    fn cmd_hash_stable_and_distinct() {
        assert_eq!(cmd_hash("docker ps"), cmd_hash("docker ps"));
        assert_ne!(cmd_hash("docker ps"), cmd_hash("docker ps -a"));
    }

    #[test]
    fn import_remote_merges_by_cmd() {
        let conn = setup();
        // 本地已有记录：used 3，无标签
        let local = db::insert_snippet(
            &conn,
            NewSnippet {
                intent: "本地版本",
                cmd: "docker ps -a",
                project: None,
                tool: Some("docker"),
                command: Some("ps"),
                variant_of: None,
                source: "self",
                verified: true,
                used_count: 3,
                last_used: None,
                notes: None,
            },
        )
        .unwrap();

        let repo = std::env::temp_dir().join(format!("snipnav-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        let snip_dir = repo.join("snippets");
        std::fs::create_dir_all(&snip_dir).unwrap();
        // 远端同命令：used 9 + 标签；另一条全新记录
        std::fs::write(
            snip_dir.join("aaaa.md"),
            export_record(0, "远端版本", "docker ps -a", None, &["docker".to_string()], 9, true),
        )
        .unwrap();
        std::fs::write(
            snip_dir.join("bbbb.md"),
            export_record(0, "远端新记录", "kubectl get pods -n prod", None, &["k8s".to_string()], 1, false),
        )
        .unwrap();

        let (added, merged) = import_remote(&conn, &repo).unwrap();
        assert_eq!(added, 1);
        assert_eq!(merged, 1);

        let (used, pinned): (i64, i64) = conn
            .query_row("SELECT used_count, pinned FROM snippets WHERE id = ?1", [local], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(used, 9, "取较大使用量");
        assert_eq!(pinned, 1, "置顶取或");
        let tags = db::snippet_tags(&conn, local);
        assert!(tags.contains(&"docker".to_string()), "并入远端标签");
        let _ = std::fs::remove_dir_all(&repo);
    }
}
