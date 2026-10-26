//! Tauri command 层：IPC 边界，薄封装核心模块。

use rusqlite::{params, Connection};
use tauri::State;

use crate::db;
use crate::explore;
use crate::importer;
use crate::models::*;
use crate::search;
use crate::tagging;
use crate::Db;

fn lock<'a>(state: &'a State<'_, Db>) -> Result<std::sync::MutexGuard<'a, Connection>, String> {
    state.inner().0.lock().map_err(|e| format!("数据库锁异常: {e}"))
}

#[tauri::command]
pub fn search(
    state: State<'_, Db>,
    q: String,
    project: Option<String>,
) -> Result<Vec<Hit>, String> {
    let conn = lock(&state)?;
    search::search(&conn, &q, project.as_deref(), 50)
}

#[tauri::command]
pub fn get_snippet(state: State<'_, Db>, id: i64) -> Result<Option<Snippet>, String> {
    let conn = lock(&state)?;
    db::load_snippet(&conn, id)
}

fn create_impl(conn: &Connection, input: SnippetInput) -> Result<Snippet, String> {
    let intent = input.intent.trim();
    let cmd = input.cmd.trim();
    if intent.is_empty() {
        return Err("人话描述不能为空".into());
    }
    if cmd.is_empty() {
        return Err("命令不能为空".into());
    }
    let (tool, command) = tagging::derive_tool_command(cmd);
    let tags = tagging::merge_tags(input.tags.as_deref(), cmd, intent);
    let project = input
        .project
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let id = db::insert_snippet(
        conn,
        db::NewSnippet {
            intent,
            cmd,
            project,
            tool: tool.as_deref(),
            command: command.as_deref(),
            variant_of: input.variant_of,
            source: "self",
            verified: true,
            used_count: 0,
            last_used: None,
            notes: input
                .notes
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        },
    )?;
    db::set_tags(conn, id, &tags)?;
    db::load_snippet(conn, id)?.ok_or_else(|| "插入后读取失败".into())
}

#[tauri::command]
pub fn create_snippet(state: State<'_, Db>, payload: SnippetInput) -> Result<Snippet, String> {
    let conn = lock(&state)?;
    create_impl(&conn, payload)
}

#[tauri::command]
pub fn update_snippet(
    state: State<'_, Db>,
    id: i64,
    payload: SnippetPatch,
) -> Result<Snippet, String> {
    let conn = lock(&state)?;
    let existing = db::load_snippet(&conn, id)?.ok_or("记录不存在")?;

    let intent = payload
        .intent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| existing.intent.clone());
    let cmd = payload
        .cmd
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| existing.cmd.clone());
    let notes = match payload.notes.as_deref() {
        Some(n) => {
            let n = n.trim();
            if n.is_empty() { None } else { Some(n.to_string()) }
        }
        None => existing.notes.clone(),
    };
    let project = match payload.project.as_deref() {
        Some(p) => {
            let p = p.trim();
            if p.is_empty() { None } else { Some(p.to_string()) }
        }
        None => existing.project.clone(),
    };
    let (tool, command) = if payload.cmd.is_some() {
        tagging::derive_tool_command(&cmd)
    } else {
        (existing.tool.clone(), existing.command.clone())
    };

    conn.execute(
        "UPDATE snippets SET intent = ?1, cmd = ?2, notes = ?3, project = ?4,
                            tool = ?5, command = ?6 WHERE id = ?7",
        params![intent, cmd, notes, project, tool, command, id],
    )
    .map_err(|e| e.to_string())?;

    if let Some(tags) = payload.tags.as_deref() {
        let merged = tagging::merge_tags(Some(tags), &cmd, &intent);
        db::set_tags(&conn, id, &merged)?;
    }
    db::load_snippet(&conn, id)?.ok_or_else(|| "更新后读取失败".into())
}

#[tauri::command]
pub fn delete_snippet(state: State<'_, Db>, id: i64) -> Result<(), String> {
    let conn = lock(&state)?;
    conn.execute("DELETE FROM interactions WHERE snippet_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    let n = conn
        .execute("DELETE FROM snippets WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("记录不存在".into());
    }
    Ok(())
}

/// 结果卡「复制」：剪贴板由前端单独调用 copy_to_clipboard 写入，
/// 这里只回写使用数据（used_count / last_used / interactions），喂给权重排序。
#[tauri::command]
pub fn copy_snippet(state: State<'_, Db>, id: i64) -> Result<Snippet, String> {
    let conn = lock(&state)?;
    let n = conn
        .execute(
            "UPDATE snippets SET used_count = used_count + 1, last_used = ?1 WHERE id = ?2",
            params![db::today(), id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("记录不存在".into());
    }
    db::log_interaction(&conn, id, "copy");
    db::load_snippet(&conn, id)?.ok_or_else(|| "读取失败".into())
}

/// 面板「复制组合」：在 copy_snippet 基础上累加 patterns 使用量。
#[tauri::command]
pub fn copy_variant(state: State<'_, Db>, id: i64, suffix: String) -> Result<(), String> {
    let conn = lock(&state)?;
    let base = db::load_snippet(&conn, id)?.ok_or("记录不存在")?;
    let suffix = suffix.trim();
    if !suffix.is_empty() {
        explore::record_pattern(&conn, &explore::base_name_of(&base), suffix)?;
    }
    conn.execute(
        "UPDATE snippets SET used_count = used_count + 1, last_used = ?1 WHERE id = ?2",
        params![db::today(), id],
    )
    .map_err(|e| e.to_string())?;
    db::log_interaction(&conn, id, "copy_variant");
    Ok(())
}

#[tauri::command]
pub fn explore(state: State<'_, Db>, id: i64) -> Result<Option<ExplorePanel>, String> {
    let conn = lock(&state)?;
    explore::get_explore(&conn, id)
}

#[tauri::command]
pub fn explore_save(
    state: State<'_, Db>,
    id: i64,
    payload: ExploreSaveInput,
) -> Result<Snippet, String> {
    let conn = lock(&state)?;
    let base = db::load_snippet(&conn, id)?.ok_or("基础记录不存在")?;
    let intent = payload
        .intent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("{}（组合）", base.intent));
    let cmd = payload.cmd.trim();
    let tags = tagging::merge_tags(payload.tags.as_deref(), cmd, &intent);
    let (tool, command) = tagging::derive_tool_command(cmd);
    let new_id = db::insert_snippet(
        &conn,
        db::NewSnippet {
            intent: &intent,
            cmd,
            project: base.project.as_deref(),
            tool: tool.as_deref(),
            command: command.as_deref(),
            variant_of: Some(id),
            source: "self",
            verified: false,
            used_count: 1,
            last_used: Some(&db::today()),
            notes: Some("由发散面板保存的组合"),
        },
    )?;
    db::set_tags(&conn, new_id, &tags)?;
    db::log_interaction(&conn, id, "save_variant");
    db::load_snippet(&conn, new_id)?.ok_or_else(|| "保存后读取失败".into())
}

#[tauri::command]
pub fn stats(state: State<'_, Db>) -> Result<Stats, String> {
    let conn = lock(&state)?;
    let mut total = 0i64;
    let mut self_c = 0i64;
    let mut tldr = 0i64;
    let mut doc = 0i64;
    {
        let mut stmt = conn
            .prepare("SELECT source, COUNT(*) FROM snippets GROUP BY source")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        for (src, n) in rows.flatten() {
            total += n;
            match src.as_str() {
                "self" => self_c = n,
                "tldr" => tldr = n,
                "doc" => doc = n,
                _ => {}
            }
        }
    }
    let mut projects: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT project FROM snippets
             WHERE project IS NOT NULL AND project != '' ORDER BY project",
        )
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    projects.truncate(20);

    Ok(Stats {
        total,
        self_count: self_c,
        tldr,
        doc,
        projects,
        data_dir: db::data_dir()?.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn tags(state: State<'_, Db>) -> Result<Vec<TagCount>, String> {
    let conn = lock(&state)?;
    let mut stmt = conn
        .prepare(
            "SELECT t.name AS name, COUNT(st.snippet_id) AS n
             FROM tags t LEFT JOIN snippet_tags st ON st.tag_id = t.id
             GROUP BY t.id ORDER BY n DESC, t.name LIMIT 60",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TagCount {
                name: r.get("name")?,
                count: r.get("n")?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn import_tldr(state: State<'_, Db>, path: String) -> Result<ImportResult, String> {
    let conn = lock(&state)?;
    let p = importer::expand_path(&path);
    importer::import_dir(&conn, &p)
}

/// v0.2：运行 `tool [command] --help` 并解析入库参数树（用户显式触发）。
#[tauri::command]
pub fn generate_params(
    state: State<'_, Db>,
    tool: String,
    command: Option<String>,
) -> Result<GenerateResult, String> {
    let conn = lock(&state)?;
    let sub = command.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (parsed, added) = crate::helpgen::generate_into(&conn, tool.trim(), sub)?;
    Ok(GenerateResult { parsed, added })
}

#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("打开剪贴板失败: {e}"))?;
    cb.set_text(text).map_err(|e| format!("写入剪贴板失败: {e}"))?;
    Ok(())
}

/// v1.1：手册页全文（doc_bodies 旁表，点开时按 id 取，不参与搜索/索引）。
#[tauri::command]
pub fn get_doc_body(state: State<'_, Db>, id: i64) -> Result<Option<String>, String> {
    let conn = lock(&state)?;
    Ok(crate::docs::get_body(&conn, id))
}

// ---------- v0.3：历史 / v0.5：AI 兜底 / 设置 / 自启 ----------

#[tauri::command]
pub fn recent_history(state: State<'_, Db>, limit: Option<usize>) -> Result<Vec<HistoryEntry>, String> {
    let conn = lock(&state)?;
    Ok(crate::history::read_recent(&conn, limit.unwrap_or(100).min(300)))
}

#[tauri::command]
pub fn llm_suggest(state: State<'_, Db>, q: String) -> Result<Vec<Snippet>, String> {
    // v1.2 P0-3：网络请求（最长 30s）绝不持有全局库锁——
    // 持锁段只做开关/配置读取与最终入库，期间搜索等其它 IPC 不被阻塞。
    let cfg = {
        let conn = lock(&state)?;
        crate::llm::prepare(&conn, &q)?
    };
    let items = crate::llm::generate(&cfg, q.trim())?;
    let conn = lock(&state)?;
    crate::llm::store(&conn, &items)
}

#[tauri::command]
pub fn get_settings(state: State<'_, Db>) -> Result<std::collections::HashMap<String, String>, String> {
    let conn = lock(&state)?;
    let mut out = std::collections::HashMap::new();
    for key in [
        crate::llm::KEY_ENABLED,
        crate::llm::KEY_BASE_URL,
        crate::llm::KEY_API_KEY,
        crate::llm::KEY_MODEL,
    ] {
        if let Some(v) = db::get_setting(&conn, key) {
            out.insert(key.to_string(), v);
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, Db>,
    values: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let conn = lock(&state)?;
    // 只接受白名单键，避免任意写
    for key in [
        crate::llm::KEY_ENABLED,
        crate::llm::KEY_BASE_URL,
        crate::llm::KEY_API_KEY,
        crate::llm::KEY_MODEL,
    ] {
        if let Some(v) = values.get(key) {
            db::set_setting(&conn, key, v.trim())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn autostart_status(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("读取自启状态失败: {e}"))
}

#[tauri::command]
pub fn autostart_enable(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .enable()
        .map_err(|e| format!("开启自启失败: {e}"))
}

#[tauri::command]
pub fn autostart_disable(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .disable()
        .map_err(|e| format!("关闭自启失败: {e}"))
}
