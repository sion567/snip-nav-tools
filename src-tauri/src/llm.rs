//! v0.5：大模型兜底生成 + 自动入库。
//!
//! 搜索无结果时用户显式点击触发；调用 OpenAI 兼容接口
//! （默认 GLM：https://open.bigmodel.cn/api/paas/v4），返回 1-3 条
//! {intent, cmd} 并以 source='ai' 入库，随后正常搜索即可见，可删。
//! API 配置存 settings 表（设置弹窗填写）；未配置时返回友好错误。

use std::time::Duration;

use rusqlite::{params, Connection};
use serde_json::json;

use crate::db::{self, NewSnippet};
use crate::models::Snippet;
use crate::tagging;

pub const KEY_ENABLED: &str = "llm_enabled";
pub const KEY_BASE_URL: &str = "llm_base_url";
pub const KEY_API_KEY: &str = "llm_api_key";
pub const KEY_MODEL: &str = "llm_model";
pub const DEFAULT_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";
pub const DEFAULT_MODEL: &str = "glm-4-flash";

const SYSTEM_PROMPT: &str = "你是命令行助手。根据用户需求给出最多 3 条可直接执行的命令，\
覆盖最可能的场景。只输出 JSON 数组，不要任何解释或代码块标记，格式：\
[{\"intent\":\"一句中文说明\",\"cmd\":\"完整命令\"}]";

#[derive(Debug)]
pub struct Suggest {
    pub intent: String,
    pub cmd: String,
}

/// 从模型回复里提取 JSON 数组（容忍 ```json 围栏 / 前后缀文本）。
pub fn extract_json_array(content: &str) -> Vec<Suggest> {
    let cleaned = content.trim().trim_matches('`');
    let start = cleaned.find('[');
    let end = cleaned.rfind(']');
    let Some((s, e)) = start.zip(end) else { return Vec::new() };
    if e <= s {
        return Vec::new();
    }
    let slice = &cleaned[s..=e];
    let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let intent = item.get("intent")?.as_str()?.trim().to_string();
            let cmd = item.get("cmd")?.as_str()?.trim().to_string();
            if intent.is_empty() || cmd.is_empty() {
                return None;
            }
            Some(Suggest { intent, cmd })
        })
        .take(3)
        .collect()
}

fn config(conn: &Connection) -> Result<(String, String, String), String> {
    let key = db::get_setting(conn, KEY_API_KEY).filter(|k| !k.trim().is_empty());
    let Some(key) = key else {
        return Err("未配置大模型 API Key（⚙️ 设置 → AI 兜底）".into());
    };
    let base = db::get_setting(conn, KEY_BASE_URL)
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let model = db::get_setting(conn, KEY_MODEL)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    Ok((base, key, model))
}

/// AI 调用配置（v1.2 P0-3：持锁读出后即可释放库锁，网络阶段不再占用）。
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// 第一步（持锁调用）：校验开关与配置。读完即可释放连接。
pub fn prepare(conn: &Connection, q: &str) -> Result<LlmConfig, String> {
    let q = q.trim();
    if q.is_empty() {
        return Err("搜索词为空".into());
    }
    // 显式开关：默认关闭，设置里勾选启用后才可用
    let enabled = db::get_setting(conn, KEY_ENABLED)
        .map(|v| v == "1")
        .unwrap_or(false);
    if !enabled {
        return Err("AI 兜底未开启（⚙️ 设置 → AI 兜底 → 勾选启用）".into());
    }
    let (base_url, api_key, model) = config(conn)?;
    Ok(LlmConfig { base_url, api_key, model })
}

/// 第二步（无锁调用）：纯网络请求 + 解析，绝不碰数据库。
pub fn generate(cfg: &LlmConfig, q: &str) -> Result<Vec<Suggest>, String> {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = json!({
        "model": cfg.model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": q },
        ],
    });

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let resp: serde_json::Value = agent
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .send_json(body)
        .map_err(|e| format!("请求失败: {e}"))?
        .into_json()
        .map_err(|e| format!("解析响应失败: {e}"))?;

    let content = resp
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or("响应里没有内容（检查模型名/额度）")?;
    let items = extract_json_array(content);
    if items.is_empty() {
        return Err("模型没有返回可用的命令建议".into());
    }
    Ok(items)
}

/// 第三步（持锁调用）：去重入库（source='ai'），返回带 id 的完整记录。
pub fn store(conn: &Connection, items: &[Suggest]) -> Result<Vec<Snippet>, String> {
    let mut out = Vec::new();
    for it in items {
        // 同问已生成过的命令不重复入库
        let dup: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM snippets WHERE source = 'ai' AND cmd = ?1",
                params![it.cmd],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
                > 0;
        if dup {
            continue;
        }
        let (tool, command) = tagging::derive_tool_command(&it.cmd);
        let id = db::insert_snippet(
            conn,
            NewSnippet {
                intent: &it.intent,
                cmd: &it.cmd,
                project: None,
                tool: tool.as_deref(),
                command: command.as_deref(),
                variant_of: None,
                source: "ai",
                verified: false,
                used_count: 0,
                last_used: None,
                notes: Some("AI 兜底生成，请确认后使用"),
            },
        )?;
        let tags = tagging::auto_tags(&it.cmd, &it.intent);
        db::set_tags(conn, id, &tags)?;
        if let Some(mut sn) = db::load_snippet(conn, id)? {
            sn.tags.sort();
            out.push(sn);
        }
    }
    Ok(out)
}

/// 一步式封装（测试用）。GUI 的 `llm_suggest` command 为避免持锁等网络
///（最长 30s 会阻塞搜索等所有 IPC），按 prepare → generate → store 三步分开调用。
pub fn suggest(conn: &Connection, q: &str) -> Result<Vec<Snippet>, String> {
    let cfg = prepare(conn, q)?;
    let items = generate(&cfg, q.trim())?;
    store(conn, &items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_and_fenced_json() {
        let plain = "[{\"intent\":\"查端口\",\"cmd\":\"netstat -ano | findstr :8080\"}]";
        assert_eq!(extract_json_array(plain).len(), 1);

        let fenced = "好的，如下：\n```json\n[{\"intent\":\"A\",\"cmd\":\"a -l\"},{\"intent\":\"B\",\"cmd\":\"b -v\"},{\"intent\":\"C\",\"cmd\":\"c\"},{\"intent\":\"D\",\"cmd\":\"d\"}]\n```\n以上。";
        let got = extract_json_array(fenced);
        assert_eq!(got.len(), 3, "最多取 3 条：{got:?}");
        assert_eq!(got[0].intent, "A");

        assert!(extract_json_array("模型拒绝回答").is_empty());
        assert!(extract_json_array("[{\"intent\":\"空cmd\"}]").is_empty());
    }

    #[test]
    fn disabled_by_default_then_key_check_after_enable() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        // 默认（无任何配置）：应提示未开启
        let err = suggest(&conn, "查大文件").unwrap_err();
        assert!(err.contains("未开启"), "应提示未开启：{err}");
        // 开启后但没配 Key：提示配置 Key
        db::set_setting(&conn, KEY_ENABLED, "1").unwrap();
        let err2 = suggest(&conn, "查大文件").unwrap_err();
        assert!(err2.contains("API Key"), "应提示配 Key：{err2}");
    }

    #[test]
    fn store_dedupes_and_persists() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        let items = vec![Suggest {
            intent: "查目录磁盘占用".into(),
            cmd: "du -sh ./* | sort -rh | head -10".into(),
        }];
        let out = store(&conn, &items).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "ai");
        assert_eq!(store(&conn, &items).unwrap().len(), 0, "同命令不重复入库");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets WHERE source='ai'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
