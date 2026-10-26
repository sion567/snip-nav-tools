//! 数据库层：连接、建表、通用读写。数据目录 ~/.snipnav（SNIPNAV_HOME 可覆盖）。

use std::path::PathBuf;

use rusqlite::{params, Connection, Row};

use crate::models::Snippet;

pub const BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS snippets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool        TEXT,
    command     TEXT,
    project     TEXT,            -- 归属项目（目录名），搜索时可加权
    intent      TEXT DEFAULT '',
    cmd         TEXT DEFAULT '',
    variant_of  INTEGER REFERENCES snippets(id) ON DELETE SET NULL,
    source      TEXT NOT NULL DEFAULT 'self',
    verified    INTEGER NOT NULL DEFAULT 1,
    used_count  INTEGER NOT NULL DEFAULT 0,
    last_used   TEXT,
    created     TEXT NOT NULL DEFAULT (date('now','localtime')),
    notes       TEXT
);
CREATE INDEX IF NOT EXISTS idx_snippets_tool ON snippets(tool, command);
-- v1.2 P0-2：热度索引——首页空查询、面板「相关命令」全局补齐、snip stats Top5
-- 都是 ORDER BY used_count DESC …，无索引时大库每次全表扫描+排序。
CREATE INDEX IF NOT EXISTS idx_snippets_hot
    ON snippets(used_count DESC, last_used DESC, id DESC);
-- v1.2 P0-4：stats 的 DISTINCT project 列表走索引扫描
CREATE INDEX IF NOT EXISTS idx_snippets_project ON snippets(project);

CREATE TABLE IF NOT EXISTS tags (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL
);

CREATE TABLE IF NOT EXISTS snippet_tags (
    snippet_id INTEGER NOT NULL REFERENCES snippets(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags(id),
    PRIMARY KEY (snippet_id, tag_id)
);

CREATE TABLE IF NOT EXISTS interactions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    snippet_id INTEGER NOT NULL,
    action     TEXT NOT NULL,
    created    TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX IF NOT EXISTS idx_interactions_snippet ON interactions(snippet_id);

CREATE TABLE IF NOT EXISTS patterns (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    base_cmd   TEXT NOT NULL,
    suffix     TEXT NOT NULL,
    used_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(base_cmd, suffix)
);

CREATE TABLE IF NOT EXISTS param_tree (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool        TEXT,
    command     TEXT,
    category    TEXT,
    flag        TEXT,
    "desc"      TEXT,
    example     TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_param_tree ON param_tree(tool, command);

-- v0.2：行数计数表（搜索时判断是否切换 FTS 检索，O(1) 读取，触发器自动维护）
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL DEFAULT 0);
INSERT OR IGNORE INTO meta(key, value) VALUES('snippet_count', (SELECT COUNT(*) FROM snippets));
CREATE TRIGGER IF NOT EXISTS snippets_count_ai AFTER INSERT ON snippets BEGIN
    UPDATE meta SET value = value + 1 WHERE key = 'snippet_count';
END;
CREATE TRIGGER IF NOT EXISTS snippets_count_ad AFTER DELETE ON snippets BEGIN
    UPDATE meta SET value = value - 1 WHERE key = 'snippet_count';
END;

-- v1.1 性能优化包 ①/④：批量文档（man 等）整页原文旁表。
-- 只在点开「全文」时按 snippet_id 取（O(1)），永不参与 LIKE 搜索，也不进 FTS 索引。
CREATE TABLE IF NOT EXISTS doc_bodies(
    snippet_id INTEGER PRIMARY KEY REFERENCES snippets(id) ON DELETE CASCADE,
    body TEXT
);

-- v0.5：应用设置（LLM 兜底配置等），键值存 SQLite，避免再引入配置文件格式
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT);
"#;

/// FTS5 表 + 同步触发器。检索路径目前走 LIKE + 权重排序，
/// 此表只为数据量上来后切换 FTS 预留，创建失败（无 FTS5）时静默降级。
pub const FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS snippets_fts USING fts5(
    intent, cmd, notes,
    content='snippets', content_rowid=id
);
CREATE TRIGGER IF NOT EXISTS snippets_fts_ai AFTER INSERT ON snippets BEGIN
    INSERT INTO snippets_fts(rowid, intent, cmd, notes)
    VALUES (new.id, new.intent, new.cmd, new.notes);
END;
CREATE TRIGGER IF NOT EXISTS snippets_fts_ad AFTER DELETE ON snippets BEGIN
    INSERT INTO snippets_fts(snippets_fts, rowid, intent, cmd, notes)
    VALUES ('delete', old.id, old.intent, old.cmd, old.notes);
END;
CREATE TRIGGER IF NOT EXISTS snippets_fts_au AFTER UPDATE ON snippets BEGIN
    INSERT INTO snippets_fts(snippets_fts, rowid, intent, cmd, notes)
    VALUES ('delete', old.id, old.intent, old.cmd, old.notes);
    INSERT INTO snippets_fts(rowid, intent, cmd, notes)
    VALUES (new.id, new.intent, new.cmd, new.notes);
END;
"#;

pub fn data_dir() -> Result<PathBuf, String> {
    if let Ok(home) = std::env::var("SNIPNAV_HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }
    let home = dirs::home_dir().ok_or("无法定位用户主目录")?;
    Ok(home.join(".snipnav"))
}

pub fn db_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("data").join("snippets.db"))
}

/// 打开（必要时创建）数据库：建表 + 种子数据。
/// 返回 `Conn` 包装：Drop 时自动做 WAL 收尾（合并并截断 -wal，v1.2 P0-1），
/// CLI 各命令结束后不再遗留几 MB 的 -wal 文件。
pub fn open() -> Result<Conn, String> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建数据目录失败: {e}"))?;
    }
    let conn =
        Connection::open(&path).map_err(|e| format!("打开数据库失败 {}: {e}", path.display()))?;
    let _ = conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()));
    // v1.2 P0-1：WAL 体积上限——checkpoint 后超过 64MB 即截断，防失控膨胀
    let _ = conn.query_row("PRAGMA journal_size_limit=67108864", [], |_| Ok(()));
    // v1.1 性能优化包 ③：大库时让扫描走内存——64MB 页缓存 + 256MB mmap（零拷贝读）
    let _ = conn.query_row("PRAGMA cache_size=-65536", [], |_| Ok(()));
    let _ = conn.query_row("PRAGMA mmap_size=268435456", [], |_| Ok(()));
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("设置外键约束失败: {e}"))?;
    init_schema(&conn)?;
    crate::seeds::seed_all(&conn)?;
    Ok(Conn::new(conn))
}

/// 带退出收尾的数据库连接（v1.2 P0-1）：Drop 时执行
/// `wal_checkpoint(TRUNCATE)` 把 -wal 合并回主库并清零。
/// `Deref` 透传 `Connection`，既有 `&conn` 调用点无需任何改动。
pub struct Conn {
    inner: Option<Connection>,
}

impl Conn {
    fn new(inner: Connection) -> Self {
        Conn { inner: Some(inner) }
    }

    /// 取出原始连接（GUI 把它放进全局状态，退出事件里另行收尾）。
    /// 取出后 Drop 不再做 checkpoint（由调用方负责）。
    pub fn into_inner(mut self) -> Connection {
        self.inner.take().expect("Conn 连接已被取空")
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Some(conn) = self.inner.as_ref() {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
    }
}

impl std::ops::Deref for Conn {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.inner.as_ref().expect("Conn 连接已被取空")
    }
}

/// 只建表不播种（测试用）。
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(BASE_SCHEMA)
        .map_err(|e| format!("建表失败: {e}"))?;
    let _ = conn.execute_batch(FTS_SCHEMA);
    Ok(())
}

pub fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn days_ago(n: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

// ---------- 通用读写 ----------

pub fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap_or(0)
}

/// 表是否无行（v1.2 P0-4：LIMIT 1 存在性判断，替代 COUNT(*) 全表计数）。
pub fn is_empty(conn: &Connection, table: &str) -> bool {
    conn.query_row(&format!("SELECT 1 FROM {table} LIMIT 1"), [], |_| Ok(()))
        .is_err()
}

/// snippets 行数（触发器维护，O(1)；表缺失时回退 COUNT）。
pub fn snippet_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT value FROM meta WHERE key = 'snippet_count'", [], |r| r.get(0))
        .unwrap_or_else(|_| {
            conn.query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))
                .unwrap_or(0)
        })
}

/// FTS5 表是否可用（构建缺 FTS5 时检索自动走 LIKE）。
pub fn has_fts(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'snippets_fts'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |r| r.get(0),
    )
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn snippet_from_row(r: &Row) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: r.get("id")?,
        tool: r.get("tool")?,
        command: r.get("command")?,
        project: r.get("project")?,
        intent: r.get::<_, Option<String>>("intent")?.unwrap_or_default(),
        cmd: r.get::<_, Option<String>>("cmd")?.unwrap_or_default(),
        variant_of: r.get("variant_of")?,
        source: r
            .get::<_, Option<String>>("source")?
            .unwrap_or_else(|| "self".into()),
        verified: r.get::<_, i64>("verified")? != 0,
        used_count: r.get("used_count")?,
        last_used: r.get("last_used")?,
        created: r
            .get::<_, Option<String>>("created")?
            .unwrap_or_default(),
        notes: r.get("notes")?,
        tags: Vec::new(),
    })
}

pub fn snippet_tags(conn: &Connection, id: i64) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN snippet_tags st ON st.tag_id = t.id
         WHERE st.snippet_id = ?1 ORDER BY t.name",
    ) else {
        return Vec::new();
    };
    stmt.query_map([id], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

pub fn attach_tags(conn: &Connection, sn: &mut Snippet) {
    sn.tags = snippet_tags(conn, sn.id);
}

pub fn load_snippet(conn: &Connection, id: i64) -> Result<Option<Snippet>, String> {
    let Ok(mut stmt) =
        conn.prepare("SELECT * FROM snippets WHERE id = ?1")
    else {
        return Ok(None);
    };
    let mut sn = match stmt
        .query_map([id], snippet_from_row)
        .map_err(|e| e.to_string())?
        .next()
    {
        Some(Ok(s)) => s,
        Some(Err(e)) => return Err(e.to_string()),
        None => return Ok(None),
    };
    attach_tags(conn, &mut sn);
    Ok(Some(sn))
}

/// 按 SQL 批量取记录（不含 tags，调用方自行 attach）。
pub fn query_snippets(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Snippet>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params, snippet_from_row)
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// 绑定标签（替换式）：统一小写、去重，tag 不存在则创建。
pub fn set_tags(conn: &Connection, snippet_id: i64, tags: &[String]) -> Result<(), String> {
    conn.execute("DELETE FROM snippet_tags WHERE snippet_id = ?1", [snippet_id])
        .map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    for raw in tags {
        let t = raw.trim().to_lowercase();
        if t.is_empty() || !seen.insert(t.clone()) {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&t])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO snippet_tags(snippet_id, tag_id)
             SELECT ?1, id FROM tags WHERE name = ?2",
            params![snippet_id, t],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub struct NewSnippet<'a> {
    pub intent: &'a str,
    pub cmd: &'a str,
    pub project: Option<&'a str>,
    pub tool: Option<&'a str>,
    pub command: Option<&'a str>,
    pub variant_of: Option<i64>,
    pub source: &'a str,
    pub verified: bool,
    pub used_count: i64,
    pub last_used: Option<&'a str>,
    pub notes: Option<&'a str>,
}

pub fn insert_snippet(conn: &Connection, n: NewSnippet) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO snippets(tool, command, project, intent, cmd, variant_of, source, verified,
                              used_count, last_used, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            n.tool,
            n.command,
            n.project,
            n.intent,
            n.cmd,
            n.variant_of,
            n.source,
            n.verified as i64,
            n.used_count,
            n.last_used,
            n.notes,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn log_interaction(conn: &Connection, snippet_id: i64, action: &str) {
    let _ = conn.execute(
        "INSERT INTO interactions(snippet_id, action) VALUES (?1, ?2)",
        params![snippet_id, action],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_empty_matches_count() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        assert!(is_empty(&conn, "snippets"));
        insert_snippet(
            &conn,
            NewSnippet {
                intent: "测试记录",
                cmd: "echo hi",
                project: None,
                tool: None,
                command: None,
                variant_of: None,
                source: "self",
                verified: true,
                used_count: 0,
                last_used: None,
                notes: None,
            },
        )
        .unwrap();
        assert!(!is_empty(&conn, "snippets"));
        assert!(is_empty(&conn, "patterns"));
    }
}
