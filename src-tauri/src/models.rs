//! IPC 数据结构：全部 camelCase 序列化，与前端 JS 字段名一致。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub tool: Option<String>,
    pub command: Option<String>,
    pub project: Option<String>,
    pub intent: String,
    pub cmd: String,
    pub variant_of: Option<i64>,
    pub source: String,
    pub verified: bool,
    pub used_count: i64,
    pub last_used: Option<String>,
    pub created: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

/// 搜索命中：记录 + 得分。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    #[serde(flatten)]
    pub sn: Snippet,
    pub score: i64,
}

// ---------- 入参 ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    pub intent: String,
    pub cmd: String,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    pub variant_of: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPatch {
    pub intent: Option<String>,
    pub cmd: Option<String>,
    pub project: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSaveInput {
    pub cmd: String,
    pub intent: Option<String>,
    pub tags: Option<Vec<String>>,
}

// ---------- 出参（面板 / 统计） ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamItem {
    pub id: i64,
    pub category: String,
    pub flag: String,
    pub desc: String,
    pub example: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamGroup {
    pub key: String,
    pub label: String,
    pub items: Vec<ParamItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub id: i64,
    pub base_cmd: String,
    pub suffix: String,
    pub used_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorePanel {
    pub base: Snippet,
    pub base_name: String,
    pub groups: Vec<ParamGroup>,
    pub patterns: Vec<Pattern>,
    pub related: Vec<Snippet>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total: i64,
    #[serde(rename = "self")]
    pub self_count: i64,
    pub tldr: i64,
    pub doc: i64,
    pub projects: Vec<String>,
    pub data_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub pages: i64,
    pub snippets: i64,
    pub skipped: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub parsed: usize,
    pub added: usize,
}

/// v0.3：shell history 条目（时间 + 命令 + 是否已入库）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub ts: String,
    pub cmd: String,
    pub saved: bool,
}
