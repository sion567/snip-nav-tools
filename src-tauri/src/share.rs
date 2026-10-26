//! v0.4：团队共享（文件级）。导出个人记录为 JSON，他人导入时按命令合并去重。
//! 内网多用户实时共用一个库（服务化）留待后续；当前模式：文件互传 / git 仓库 / 网盘同步。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::{self, NewSnippet};
use crate::tagging;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareEntry {
    pub intent: String,
    pub cmd: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub used_count: i64,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareFile {
    pub version: u32,
    pub exported: String,
    pub entries: Vec<ShareEntry>,
}

/// 导出全部个人记录（source = self）。
pub fn export_self(conn: &Connection, path: &std::path::Path) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT id, intent, cmd, project, used_count, notes FROM snippets WHERE source = 'self' ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in rows.flatten() {
        let (id, intent, cmd, project, used_count, notes) = row;
        let tags = db::snippet_tags(conn, id);
        entries.push(ShareEntry {
            intent,
            cmd,
            project,
            tags,
            used_count,
            notes,
        });
    }
    let n = entries.len();
    let file = ShareFile {
        version: 1,
        exported: db::today(),
        entries,
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("写入失败: {e}"))?;
    Ok(n)
}

pub struct ImportStat {
    pub total: usize,
    pub added: usize,
    pub merged: usize,
}

/// 导入共享文件：同命令（self/ai）已存在则合并（取较大使用量、并标签），否则新增。
pub fn import_share(conn: &Connection, path: &std::path::Path) -> Result<ImportStat, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    let file: ShareFile = serde_json::from_str(&json).map_err(|e| format!("文件格式不正确: {e}"))?;
    let mut stat = ImportStat { total: file.entries.len(), added: 0, merged: 0 };
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    for e in &file.entries {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM snippets WHERE cmd = ?1 AND source IN ('self','ai') LIMIT 1",
                params![e.cmd],
                |r| r.get(0),
            )
            .ok();
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE snippets SET used_count = MAX(used_count, ?2) WHERE id = ?1",
                    params![id, e.used_count],
                )
                .map_err(|er| er.to_string())?;
                let mut tags = db::snippet_tags(conn, id);
                for t in &e.tags {
                    if !tags.contains(t) {
                        tags.push(t.clone());
                    }
                }
                db::set_tags(conn, id, &tags)?;
                stat.merged += 1;
            }
            None => {
                let (tool, command) = tagging::derive_tool_command(&e.cmd);
                let new_id = db::insert_snippet(
                    conn,
                    NewSnippet {
                        intent: &e.intent,
                        cmd: &e.cmd,
                        project: e.project.as_deref(),
                        tool: tool.as_deref(),
                        command: command.as_deref(),
                        variant_of: None,
                        source: "self",
                        verified: true,
                        used_count: e.used_count,
                        last_used: None,
                        notes: e.notes.as_deref(),
                    },
                )?;
                let mut tags = e.tags.clone();
                tags.extend(tagging::auto_tags(&e.cmd, &e.intent));
                db::set_tags(conn, new_id, &tags)?;
                stat.added += 1;
            }
        }
    }
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    Ok(stat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip_merges() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        crate::seeds::seed_all(&conn).unwrap();

        let dir = std::env::temp_dir().join(format!("snipnav-share-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("share.json");

        let n = export_self(&conn, &file).unwrap();
        assert!(n >= 6, "种子个人记录应全部导出");

        // 同库重导入：全部命中同命令 → merged，无新增
        let st = import_share(&conn, &file).unwrap();
        assert_eq!(st.added, 0);
        assert_eq!(st.merged, n);
        assert_eq!(db::count(&conn, "snippets"), 29, "总量不应变化");

        // 改一份"别人的"文件：改 intent/加新条目 → 新增
        let mut sf: ShareFile = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        sf.entries.push(ShareEntry {
            intent: "队友的记录".to_string(),
            cmd: "ffmpeg -i in.mp4 -vn -acodec copy out.m4a".to_string(),
            project: Some("media-tool".to_string()),
            tags: vec!["media".to_string()],
            used_count: 3,
            notes: None,
        });
        std::fs::write(&file, serde_json::to_string(&sf).unwrap()).unwrap();
        let st2 = import_share(&conn, &file).unwrap();
        assert_eq!(st2.added, 1);
        assert_eq!(st2.merged, n);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
