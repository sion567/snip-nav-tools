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
    pub pinned: bool,
    /// v1.8：意图拼音（搜全拼/缩写用），前端不需要，不出序列化
    #[serde(skip_serializing)]
    pub intent_py: String,
    #[serde(skip_serializing)]
    pub intent_pyi: String,
    pub used_count: i64,
    pub last_used: Option<String>,
    pub created: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    /// v1.6：危险命令原因（读行时按规则即时计算，不落库；None = 无标记）
    pub danger: Option<String>,
}

/// 搜索命中：记录 + 得分。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    #[serde(flatten)]
    pub sn: Snippet,
    pub score: i64,
}

/// 文档库浏览：按来源分组的完整列表（v1.4）。
/// 与 search 的分工——search 求相关性（权重排序 + 截断），browse 求完整性（全源、分组、不截断）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocGroup {
    pub key: String, // self / ai / tldr / doc
    pub label: String,
    pub items: Vec<Snippet>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocBrowse {
    pub query: String,
    pub total: i64,
    pub groups: Vec<DocGroup>,
    /// 空查询时的工具名索引（工具名 + 条数），作为浏览落地页
    pub tools: Vec<TagCount>,
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

// ---------- v1.7：Playbook 运行手册 ----------

/// 手册（有序命令序列），steps 按顺序带出完整记录（含 ⚠️ danger）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playbook {
    pub id: i64,
    pub name: String,
    pub note: Option<String>,
    pub created: String,
    pub steps: Vec<Snippet>,
}

/// 新建/编辑手册：step_ids 顺序即步骤顺序（替换式重写）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookInput {
    pub id: Option<i64>,
    pub name: String,
    pub note: Option<String>,
    pub step_ids: Vec<i64>,
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

// ---------- v1.7：命令解释（explainshell 式逐段标注） ----------

/// 一段命令的组成片段：flag 型片段带参数树描述（desc=None 表示参数树里没有）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdPart {
    pub text: String,
    pub desc: Option<String>,
}

/// 按管道分段后的解释：每段 = 该段工具 + 空格切分的片段列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainSeg {
    pub tool: String,
    pub parts: Vec<CmdPart>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorePanel {
    pub base: Snippet,
    pub base_name: String,
    pub groups: Vec<ParamGroup>,
    pub patterns: Vec<Pattern>,
    pub related: Vec<Snippet>,
    /// v1.7：命令逐段解释（管道分段 + flag 查参数树）
    pub explain: Vec<ExplainSeg>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

/// v1.9 使用热力图：某天的复制/保存动作次数（近一年，稀疏返回）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCount {
    pub date: String, // YYYY-MM-DD
    pub count: i64,
}

/// v1.9 工作流导入结果（Warp / Espanso / VS Code 统一）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowImportResult {
    pub files: i64,
    pub snippets: i64,
    pub skipped: i64,
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
    /// v1.5：来源 shell（"bash" / "powershell"）；PowerShell 条目 ts 为空
    pub shell: String,
}
