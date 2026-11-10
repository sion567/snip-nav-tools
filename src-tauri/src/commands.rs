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

/// v1.4 文档库浏览：全源、分组、不截断（与 search 的相关性排序互补）。
#[tauri::command]
pub fn browse_docs(state: State<'_, Db>, q: String) -> Result<DocBrowse, String> {
    let conn = lock(&state)?;
    search::browse(&conn, &q, search::BROWSE_LIMIT)
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
        "UPDATE snippets SET intent = ?1, intent_py = ?2, intent_pyi = ?3,
                            cmd = ?4, notes = ?5, project = ?6,
                            tool = ?7, command = ?8 WHERE id = ?9",
        params![
            intent,
            tagging::pinyin_full(&intent),
            tagging::pinyin_initials(&intent),
            cmd,
            notes,
            project,
            tool,
            command,
            id
        ],
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

/// v1.5 收藏置顶：卡片 📌 一键切换，排序权重恒置顶。
#[tauri::command]
pub fn set_pinned(state: State<'_, Db>, id: i64, pinned: bool) -> Result<Snippet, String> {
    let conn = lock(&state)?;
    let n = conn
        .execute(
            "UPDATE snippets SET pinned = ?1 WHERE id = ?2",
            params![pinned as i64, id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("记录不存在".into());
    }
    db::log_interaction(&conn, id, if pinned { "pin" } else { "unpin" });
    db::load_snippet(&conn, id)?.ok_or_else(|| "读取失败".into())
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

/// 顶栏徽标读真实版本（单一来源 tauri.conf.json，避免硬编码漂移）。
// ---------- v1.7：Playbook 运行手册 ----------

fn load_playbook(conn: &Connection, id: i64) -> Result<Playbook, String> {
    let (name, note, created) = conn
        .query_row(
            "SELECT name, note, IFNULL(created,'') FROM playbooks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "手册不存在".to_string())?;
    let mut steps = db::query_snippets(
        conn,
        "SELECT s.* FROM snippets s JOIN playbook_items pi ON pi.snippet_id = s.id
         WHERE pi.playbook_id = ?1 ORDER BY pi.position, pi.id",
        &[&id as &dyn rusqlite::ToSql],
    )?;
    for s in steps.iter_mut() {
        db::attach_tags(conn, s);
    }
    Ok(Playbook { id, name, note, created, steps })
}

/// 全部手册（按 id 倒序 = 新建的在前），steps 按 position 序带出。
#[tauri::command]
pub fn playbooks(state: State<'_, Db>) -> Result<Vec<Playbook>, String> {
    let conn = lock(&state)?;
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM playbooks ORDER BY id DESC")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    ids.iter().map(|&id| load_playbook(&conn, id)).collect()
}

/// 新建/编辑手册：step_ids 顺序即步骤顺序，替换式重写步骤。
#[tauri::command]
pub fn playbook_save(state: State<'_, Db>, payload: PlaybookInput) -> Result<Playbook, String> {
    let conn = lock(&state)?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err("手册名称不能为空".into());
    }
    let id = match payload.id {
        Some(id) => {
            let note = payload.note.as_deref().map(str::trim).filter(|s| !s.is_empty());
            let n = conn
                .execute(
                    "UPDATE playbooks SET name = ?2, note = ?3 WHERE id = ?1",
                    params![id, name, note],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("手册不存在".into());
            }
            id
        }
        None => {
            let note = payload.note.as_deref().map(str::trim).filter(|s| !s.is_empty());
            conn.execute(
                "INSERT INTO playbooks(name, note) VALUES (?1, ?2)",
                params![name, note],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    conn.execute("DELETE FROM playbook_items WHERE playbook_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    for (pos, sid) in payload.step_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO playbook_items(playbook_id, snippet_id, position) VALUES (?1, ?2, ?3)",
            params![id, sid, pos as i64],
        )
        .map_err(|e| format!("步骤写入失败（记录 #{} 可能已被删除）: {e}", sid))?;
    }
    load_playbook(&conn, id)
}

#[tauri::command]
pub fn playbook_delete(state: State<'_, Db>, id: i64) -> Result<(), String> {
    let conn = lock(&state)?;
    // items 由外键级联删除
    let n = conn
        .execute("DELETE FROM playbooks WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("手册不存在".into());
    }
    Ok(())
}

// ---------- v1.8：复制并粘贴回原窗口 ----------

pub const KEY_QUICK_PASTE: &str = "quick_paste";

/// 浮窗 Enter（粘贴模式开启时）：写剪贴板 → 藏浮窗（焦点自然回到呼出前的
/// 窗口）→ 320ms 后向目标窗口模拟 Ctrl+V。终端里仍由用户自己按回车执行。
#[tauri::command]
pub fn paste_and_hide(
    app: tauri::AppHandle,
    target: State<'_, crate::paste::PasteTarget>,
    text: String,
) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("打开剪贴板失败: {e}"))?;
    cb.set_text(text).map_err(|e| format!("写入剪贴板失败: {e}"))?;
    {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("quick") {
            let _ = w.hide();
        }
    }
    let hwnd = target
        .0
        .lock()
        .map_err(|e| format!("状态锁异常: {e}"))?
        .clone();
    crate::paste::send_paste(hwnd);
    Ok(())
}

/// v1.9 使用热力图：近一年每天的动作次数（复制/组合/保存变体/收藏）。
#[tauri::command]
pub fn heatmap(state: State<'_, Db>) -> Result<Vec<DayCount>, String> {
    let conn = lock(&state)?;
    let mut stmt = conn
        .prepare(
            "SELECT date(created) AS d, COUNT(*) FROM interactions
             WHERE date(created) >= date('now', '-1 year', 'localtime')
             GROUP BY d ORDER BY d",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DayCount {
                date: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn import_workflows(state: State<'_, Db>, path: String) -> Result<WorkflowImportResult, String> {
    let conn = lock(&state)?;
    let p = importer::expand_path(&path);
    crate::workflows::import_path(&conn, &p)
}

/// v1.10：探测端点上的模型列表（设置页「检测本地 Ollama」用，前端传当前填的 base_url）。
/// 纯网络请求不持锁。
#[tauri::command]
pub fn llm_models(base_url: String) -> Result<Vec<String>, String> {
    let b = base_url.trim();
    if b.is_empty() {
        return Err("先填 API Base URL".into());
    }
    crate::llm::list_models(b)
}

#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
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

// ---------- v1.5：速查浮窗 ----------

pub const KEY_QUICK_HOTKEY: &str = "quick_hotkey";

/// quick 页复制完成后 / Esc 时自行隐藏浮窗。
#[tauri::command]
pub fn quick_hide(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("quick") {
        let _ = w.hide();
    }
    Ok(())
}

/// 设置页修改全局快捷键：立即重注册并持久化（空串 = 禁用快捷键）。
#[tauri::command]
pub fn set_quick_hotkey(
    app: tauri::AppHandle,
    state: State<'_, Db>,
    hotkey: String,
) -> Result<(), String> {
    let h = hotkey.trim().to_string();
    crate::register_quick_hotkey(&app, &h)?;
    let conn = lock(&state)?;
    db::set_setting(&conn, KEY_QUICK_HOTKEY, &h)
}

// ---------- v1.6：剪贴板监听 ----------

/// 前端轮询取「待入库」的剪贴板命令（无则 None）。peek 语义：展示中不被清掉。
#[tauri::command]
pub fn pending_clipboard(state: State<'_, crate::clipboard::Pending>) -> Result<Option<crate::clipboard::PendingCmd>, String> {
    let p = state
        .0
        .lock()
        .map_err(|e| format!("状态锁异常: {e}"))?;
    Ok(p.clone())
}

/// 用户点「忽略」或保存成功后清掉待入库命令。
#[tauri::command]
pub fn dismiss_pending_clipboard(state: State<'_, crate::clipboard::Pending>) -> Result<(), String> {
    let mut p = state
        .0
        .lock()
        .map_err(|e| format!("状态锁异常: {e}"))?;
    *p = None;
    Ok(())
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
        KEY_QUICK_HOTKEY,
        KEY_QUICK_PASTE,
        crate::clipboard::KEY_CLIPBOARD_WATCH,
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
        crate::clipboard::KEY_CLIPBOARD_WATCH,
        KEY_QUICK_PASTE,
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
