// SnipNav 界面 —— Vue3 全局构建（无编译步骤）
// 注意：模板表达式经 new Function 编译，只能看到全局作用域，
// 因此 store 与所有函数都在顶层声明，不要用 IIFE 包裹本文件。

const { createApp, reactive, computed, watch } = Vue;

const QUICK_TAGS = ['docker', 'git', 'log', 'filter', 'network', 'k8s', 'gradle', 'maven', 'sql', 'windows'];

function emptyForm() {
  return { intent: '', cmd: '', tags: '', project: '', notes: '', variantOf: null };
}

const store = reactive({
  q: '',
  project: '',                 // '' = 全部项目
  theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
  results: [],
  searching: false,
  selected: null,
  panel: null,
  composed: [],                // [{key, text, desc}]
  stats: { total: 0, self: 0, tldr: 0, doc: 0, projects: [], dataDir: '' },
  appVersion: '',               // 顶栏徽标（IPC 读 tauri.conf.json 版本，避免硬编码漂移）
  tagList: [],
  toast: '',
  dialog: { open: false, editing: null, form: emptyForm() },
  settings: { open: false, importPath: '', importing: false, message: '',
              llm: { enabled: false, baseUrl: '', apiKey: '', model: '' }, llmSaved: '', autostart: null, autostartBusy: false },
  llmEnabled: false,            // AI 兜底总开关（默认关闭，设置里开启）
  historyModal: { open: false, list: [], loading: false },
  aiLoading: false,
  docBody: { open: false, title: '', text: '', loading: false },
});

// 挂到 store 上的 computed（模板经 reactive 代理自动解包）
store.quickTags = computed(() => {
  const seen = new Set();
  const out = [];
  // 动态高频在前：静态常量只补位，不再挤占库里实际常用的标签
  for (const t of store.tagList.slice(0, 8).concat(QUICK_TAGS)) {
    if (!seen.has(t)) { seen.add(t); out.push(t); }
  }
  return out.slice(0, 12);
});
store.composedCmd = computed(() => {
  const base = store.panel ? store.panel.base.cmd : '';
  if (!store.composed.length) return base;
  const parts = store.composed.map((c) => c.text.trim()).filter(Boolean);
  // 非管道参数（flag）插到第一个管道之前，管道片段接在末尾，保证组合是合法命令
  const inline = parts.filter((t) => !t.startsWith('|'));
  const pipes = parts.filter((t) => t.startsWith('|'));
  const cut = base.indexOf('|');
  const head = (cut >= 0 ? base.slice(0, cut) : base).trim();
  const tail = cut >= 0 ? base.slice(cut).trim() : '';
  return [inline.length ? head + ' ' + inline.join(' ') : head, tail, ...pipes]
    .filter(Boolean)
    .join(' ');
});
store.canSave = computed(() => {
  const f = store.dialog.form;
  return f.intent.trim().length > 0 && f.cmd.trim().length > 0;
});

// ---------- 主题 ----------
function toggleTheme() {
  store.theme = store.theme === 'light' ? 'dark' : 'light';
  if (store.theme === 'light') document.documentElement.dataset.theme = 'light';
  else delete document.documentElement.dataset.theme; // 暗黑是 CSS 默认
  try { localStorage.setItem('snipnav-theme', store.theme); } catch (e) { /* 隐私模式等场景忽略 */ }
}

// ---------- 通用 ----------
let toastTimer = null;
function showToast(msg) {
  store.toast = String(msg);
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { store.toast = ''; }, 1800);
}

// ---------- 搜索 ----------
let searchTimer = null;
watch(() => store.q, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(doSearch, 220);
});

async function doSearch() {
  store.searching = true;
  try {
    store.results = await SNIP.invoke('search', { q: store.q, project: store.project || null });
  } catch (e) {
    showToast('搜索失败：' + e);
  } finally {
    store.searching = false;
  }
}

function setProject(p) {
  if (store.project === p) return;
  store.project = p;
  doSearch();
}

async function refreshStats() {
  try { Object.assign(store.stats, await SNIP.invoke('stats')); } catch (e) { /* 忽略 */ }
  try { store.appVersion = await SNIP.invoke('app_version'); } catch (e) { /* 忽略 */ }
}
async function refreshTags() {
  try { store.tagList = (await SNIP.invoke('tags')).map((t) => t.name); } catch (e) { /* 忽略 */ }
}

// ---------- 复制 ----------
async function copyText(text) {
  try {
    await SNIP.invoke('copy_to_clipboard', { text });
    return true;
  } catch (e) {
    try { await navigator.clipboard.writeText(text); return true; } catch (e2) { /* 继续兜底 */ }
    const ta = document.createElement('textarea');
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    let ok = false;
    try { ok = document.execCommand('copy'); } catch (e3) { ok = false; }
    ta.remove();
    return ok;
  }
}

async function copyResult(r) {
  if (!(await copyText(r.cmd))) return showToast('复制失败');
  try {
    const upd = await SNIP.invoke('copy_snippet', { id: r.id });
    Object.assign(r, upd);
    if (store.panel && store.selected && store.selected.id === r.id) store.panel.base = upd;
  } catch (e) { /* 使用量回写失败不影响复制 */ }
  showToast('已复制 · ' + r.intent);
}

// ---------- 发散面板 ----------
async function select(r) {
  store.selected = r;
  store.composed = [];
  store.panel = null;
  try {
    store.panel = await SNIP.invoke('explore', { id: r.id });
  } catch (e) {
    showToast('面板加载失败：' + e);
  }
}

function closePanel() {
  store.panel = null;
  store.selected = null;
  store.composed = [];
}

function isActive(key) { return store.composed.some((c) => c.key === key); }

function togglePart(p) {
  toggleComposed('p' + p.id, (p.example || p.flag || '').trim(), p.desc || '');
}
function toggleSuffix(p) {
  toggleComposed('s' + p.id, (p.suffix || '').trim(), '常用组合');
}
function toggleComposed(key, text, desc) {
  if (!text) return;
  const i = store.composed.findIndex((c) => c.key === key);
  if (i >= 0) store.composed.splice(i, 1);
  else store.composed.push({ key, text, desc });
}

async function copyComposed() {
  const cmd = store.composedCmd;
  if (!cmd) return;
  if (!(await copyText(cmd))) return showToast('复制失败');
  const suffix = store.composed.map((c) => c.text).join(' ');
  try {
    if (store.panel) {
      if (suffix) await SNIP.invoke('copy_variant', { id: store.panel.base.id, suffix });
      else await SNIP.invoke('copy_snippet', { id: store.panel.base.id });
    }
  } catch (e) { /* 忽略回写失败 */ }
  showToast('已复制组合');
}

function saveComposed() {
  const base = store.panel.base;
  openDialog({
    intent: base.intent + '（组合）',
    cmd: store.composedCmd,
    tags: (base.tags || []).join(','),
    project: base.project || '',
    notes: '',
    variantOf: base.id,
  });
}

function shortCmd(cmd) {
  return cmd && cmd.length > 46 ? cmd.slice(0, 44) + '…' : (cmd || '');
}

// v0.2：--help 抓取生成参数树（工具名白名单校验在 Rust 侧）
async function genParams() {
  const base = store.panel.base;
  if (!base || !base.tool) return;
  try {
    const r = await SNIP.invoke('generate_params', { tool: base.tool, command: base.command || null });
    showToast('已从 --help 解析 ' + r.parsed + ' 个参数，新增 ' + r.added + ' 个');
    // 重新加载面板以显示新参数
    store.panel = await SNIP.invoke('explore', { id: base.id });
    store.composed = [];
  } catch (e) {
    showToast('生成失败：' + e);
  }
}

// ---------- 新记录 / 编辑 ----------
function openDialog(form) {
  store.dialog.form = Object.assign(emptyForm(), form || null);
  store.dialog.editing = null;
  store.dialog.open = true;
}
function openCreate() { openDialog(null); }

function openEdit(r) {
  store.dialog.editing = r;
  store.dialog.form = {
    intent: r.intent || '',
    cmd: r.cmd || '',
    tags: (r.tags || []).join(','),
    project: r.project || '',
    notes: r.notes || '',
    variantOf: r.variantOf || null,
  };
  store.dialog.open = true;
}
function closeDialog() { store.dialog.open = false; }

async function saveDialog() {
  if (!store.canSave) return;
  const f = store.dialog.form;
  const tags = f.tags.trim() ? f.tags.split(/[,，\s]+/).map((t) => t.trim()).filter(Boolean) : null;
  try {
    if (store.dialog.editing) {
      const upd = await SNIP.invoke('update_snippet', {
        id: store.dialog.editing.id,
        payload: {
          intent: f.intent.trim(),
          cmd: f.cmd.trim(),
          project: f.project.trim(),
          notes: f.notes.trim(),
          tags: tags || [],
        },
      });
      Object.assign(store.dialog.editing, upd);
    } else {
      await SNIP.invoke('create_snippet', {
        payload: {
          intent: f.intent.trim(),
          cmd: f.cmd.trim(),
          project: f.project.trim() || null,
          tags: tags,
          notes: f.notes.trim() || null,
          variantOf: f.variantOf,
        },
      });
    }
    store.dialog.open = false;
    showToast('已保存');
    doSearch();
    refreshStats();
    refreshTags();
    if (store.historyModal.open) refreshHistory();
  } catch (e) {
    showToast('保存失败：' + e);
  }
}

async function removeSnippet(r) {
  if (!window.confirm('删除这条记录？\n\n' + r.intent + '\n' + r.cmd)) return;
  try {
    await SNIP.invoke('delete_snippet', { id: r.id });
    store.results = store.results.filter((x) => x.id !== r.id);
    if (store.selected && store.selected.id === r.id) closePanel();
    refreshStats();
    refreshTags();
    showToast('已删除');
  } catch (e) {
    showToast('删除失败：' + e);
  }
}

// ---------- 设置 / 导入 ----------
async function refreshLlmFlag() {
  try {
    const s = await SNIP.invoke('get_settings');
    store.llmEnabled = s.llm_enabled === '1';
  } catch (e) { /* 忽略 */ }
}

async function openSettings() {
  store.settings.open = true;
  try {
    const s = await SNIP.invoke('get_settings');
    store.settings.llm = {
      enabled: s.llm_enabled === '1',
      baseUrl: s.llm_base_url || '',
      apiKey: s.llm_api_key || '',
      model: s.llm_model || '',
    };
    store.llmEnabled = store.settings.llm.enabled;
  } catch (e) { /* 忽略 */ }
  try { store.settings.autostart = await SNIP.invoke('autostart_status'); } catch (e) { store.settings.autostart = null; }
}

async function toggleLlm() {
  const next = !store.settings.llm.enabled;
  try {
    await SNIP.invoke('save_settings', { values: { llm_enabled: next ? '1' : '0' } });
    store.settings.llm.enabled = next;
    store.llmEnabled = next;
    showToast(next ? 'AI 兜底已开启（记得配置 API Key）' : 'AI 兜底已关闭');
  } catch (e) {
    showToast('切换失败：' + e);
  }
}

async function saveLlm() {
  const llm = store.settings.llm;
  try {
    await SNIP.invoke('save_settings', { values: {
      llm_base_url: llm.baseUrl, llm_api_key: llm.apiKey, llm_model: llm.model,
    } });
    store.settings.llmSaved = '✅ 已保存';
    setTimeout(() => { store.settings.llmSaved = ''; }, 1500);
  } catch (e) {
    store.settings.llmSaved = '❌ ' + e;
  }
}

async function toggleAutostart() {
  store.settings.autostartBusy = true;
  try {
    if (store.settings.autostart) await SNIP.invoke('autostart_disable');
    else await SNIP.invoke('autostart_enable');
    store.settings.autostart = !(store.settings.autostart === true);
  } catch (e) {
    showToast('切换自启失败：' + e);
  } finally {
    store.settings.autostartBusy = false;
  }
}

// ---------- v0.3：历史 ----------
async function openHistory() {
  store.historyModal.open = true;
  store.historyModal.loading = true;
  try {
    store.historyModal.list = await SNIP.invoke('recent_history', { limit: 100 });
  } catch (e) {
    showToast('读取历史失败：' + e);
  } finally {
    store.historyModal.loading = false;
  }
}

function adoptFromHistory(e) {
  openDialog({ intent: '', cmd: e.cmd, tags: '', project: '', notes: '', variantOf: null });
}

// v1.1：手册页全文（doc_bodies 旁表，按需取，不参与搜索）
async function openDocBody(r) {
  store.docBody = { open: true, title: r.intent, text: '', loading: true };
  try {
    const text = await SNIP.invoke('get_doc_body', { id: r.id });
    store.docBody.text = text || '（无全文内容）';
  } catch (e) {
    store.docBody.text = '读取失败：' + e;
  } finally {
    store.docBody.loading = false;
  }
}

async function refreshHistory() {
  try { store.historyModal.list = await SNIP.invoke('recent_history', { limit: 100 }); } catch (e2) { /* 忽略 */ }
}

// ---------- v0.5：AI 兜底 ----------
async function aiSuggest() {
  if (store.aiLoading || !store.q.trim()) return;
  store.aiLoading = true;
  try {
    const list = await SNIP.invoke('llm_suggest', { q: store.q });
    showToast('AI 生成 ' + list.length + ' 条并已入库，可继续搜索或删除');
    await doSearch();
  } catch (e) {
    showToast(String(e));
  } finally {
    store.aiLoading = false;
  }
}

async function doImport() {
  const p = store.settings.importPath.trim();
  if (!p) return;
  store.settings.importing = true;
  store.settings.message = '';
  try {
    const r = await SNIP.invoke('import_tldr', { path: p });
    store.settings.message = '✅ 导入完成：' + r.pages + ' 个页面文件，新增 ' + r.snippets + ' 条速查，跳过重复 ' + r.skipped + ' 条';
    doSearch();
    refreshStats();
    refreshTags();
  } catch (e) {
    store.settings.message = '❌ ' + e;
  } finally {
    store.settings.importing = false;
  }
}

// ---------- 快捷键 ----------
function isTyping(el) {
  return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
}
window.addEventListener('keydown', (e) => {
  if (e.key === '/' && !isTyping(e.target)) {
    e.preventDefault();
    const input = document.querySelector('.search-row input');
    if (input) input.focus();
  } else if (e.key === 'Escape') {
    if (store.dialog.open) store.dialog.open = false;
    else if (store.settings.open) store.settings.open = false;
    else if (store.panel) closePanel();
  }
});

// ---------- 组件模板 ----------
const CARD_TPL = `
<div class="card" :class="{ selected: store.selected && store.selected.id === r.id }">
  <div class="card-top">
    <span class="badge" :class="r.source">
      <template v-if="r.source === 'self'">🟢 你用过 {{r.usedCount}} 次</template>
      <template v-else-if="r.source === 'tldr'">📖 tldr 中文</template>
      <template v-else-if="r.source === 'ai'">🤖 AI 生成</template>
      <template v-else>📄 官方文档</template>
    </span>
    <span class="proj" v-if="r.project" title="按此项目过滤" @click="setProject(r.project)">📁 {{r.project}}</span>
    <span class="muted small" v-if="r.lastUsed">上次 {{r.lastUsed}}</span>
    <span class="muted small" v-if="r.variantOf" title="发散面板保存的组合">🔀 组合</span>
  </div>
  <div class="intent">{{r.intent}}</div>
  <pre class="cmd">{{r.cmd}}</pre>
  <div class="card-meta">
    <span class="tag" v-for="t in r.tags" :key="t" title="按此标签搜索" @click="store.q = t">🏷 {{t}}</span>
    <span class="muted small notes" v-if="r.notes" :title="r.notes">📝 {{r.notes}}</span>
  </div>
  <div class="card-actions">
    <button class="btn primary sm" @click="copyResult(r)">📋 复制</button>
    <button class="btn sm" @click="select(r)">🔀 展开变体</button>
    <button v-if="r.source === 'doc'" class="btn sm" @click="openDocBody(r)">📖 全文</button>
    <template v-if="r.source === 'self'">
      <button class="btn sm" title="编辑" @click="openEdit(r)">✏️</button>
      <button class="btn sm danger" title="删除" @click="removeSnippet(r)">🗑</button>
    </template>
  </div>
</div>
`;

const PANEL_TPL = `
<div class="explore">
  <div class="explore-head">
    <div class="muted small">基于 {{store.panel.baseName}}</div>
    <pre class="cmd">{{store.panel.base.cmd}}</pre>
    <div class="row" style="justify-content: space-between; align-items: center;">
      <span class="muted small tip">点击参数 / 组合拼进「当前组合」</span>
      <button v-if="store.panel.base.tool" class="btn sm ghost" title="运行 工具 --help 解析参数，自动补进参数树（增量，可重复）" @click="genParams">⚡ --help 补充参数</button>
    </div>
  </div>

  <div class="composer" v-if="store.composed.length">
    <div class="muted small">当前组合</div>
    <pre class="cmd composed">{{store.composedCmd}}</pre>
    <div class="row">
      <button class="btn primary sm" @click="copyComposed">📋 复制组合</button>
      <button class="btn sm" @click="saveComposed">💾 存为新记录</button>
      <button class="btn sm ghost" @click="store.composed = []">✕ 清空</button>
    </div>
  </div>

  <div class="group" v-for="g in store.panel.groups" :key="'g-' + g.key">
    <h4>▸ {{g.label}}</h4>
    <div class="param" v-for="p in g.items" :key="'p-' + p.id" :class="{ on: isActive('p' + p.id) }" @click="togglePart(p)">
      <code>{{p.flag}}</code><span class="pdesc">{{p.desc}}</span>
    </div>
  </div>

  <div class="group" v-if="store.panel.patterns.length">
    <h4>▸ 你的常用组合</h4>
    <div class="param" v-for="p in store.panel.patterns" :key="'pt-' + p.id" :class="{ on: isActive('s' + p.id) }" @click="toggleSuffix(p)">
      <code>{{p.suffix}}</code><span class="pdesc">用过 {{p.usedCount}} 次</span>
    </div>
  </div>

  <div class="group" v-if="store.panel.related.length">
    <h4>▸ 相关命令</h4>
    <div class="related" v-for="rel in store.panel.related" :key="'rel-' + rel.id" @click="select(rel)">
      <code>{{shortCmd(rel.cmd)}}</code><span class="pdesc">{{rel.intent}}</span>
    </div>
  </div>

  <button class="btn ghost sm block" @click="closePanel">收起面板 ✕</button>
</div>
`;

const DIALOG_TPL = `
<div class="overlay" v-if="store.dialog.open" @click.self="closeDialog">
  <div class="modal">
    <h3>{{store.dialog.editing ? '✏️ 编辑记录 #' + store.dialog.editing.id : '＋ 新记录'}}</h3>
    <label>人话描述 <i class="muted small">（搜索靠它，用你自己的话）</i>
      <input v-model="store.dialog.form.intent" placeholder="例：docker 日志按时间过滤错误" />
    </label>
    <label>完整命令
      <textarea v-model="store.dialog.form.cmd" rows="3" placeholder="docker logs -f mycontainer --since 1h | grep ERROR"></textarea>
    </label>
    <div class="form-grid">
      <label>项目
        <input v-model="store.dialog.form.project" list="sn-project-list" placeholder="留空 = 不归属" />
      </label>
      <label>标签 <i class="muted small">（逗号分隔，留空自动打）</i>
        <input v-model="store.dialog.form.tags" placeholder="docker, log, filter" />
      </label>
    </div>
    <datalist id="sn-project-list">
      <option v-for="p in store.stats.projects" :key="'dl-' + p" :value="p"></option>
    </datalist>
    <label>备注
      <textarea v-model="store.dialog.form.notes" rows="2" placeholder="注意事项，可不填"></textarea>
    </label>
    <div class="row end">
      <button class="btn" @click="closeDialog">取消</button>
      <button class="btn primary" :disabled="!store.canSave" @click="saveDialog">{{store.dialog.editing ? '保存修改' : '保存'}}</button>
    </div>
  </div>
</div>
`;

const SETTINGS_TPL = `
<div class="overlay" v-if="store.settings.open" @click.self="store.settings.open = false">
  <div class="modal">
    <h3>⚙️ 设置</h3>
    <div class="kv"><span>运行模式</span><span>{{SNIP.isTauri ? 'Tauri 桌面' : '浏览器 Mock（开发）'}}</span></div>
    <div class="kv"><span>数据目录</span><span class="small">{{store.stats.dataDir || '…'}}</span></div>
    <div class="kv"><span>记录统计</span><span>个人 {{store.stats.self}} · tldr {{store.stats.tldr}} · 文档 {{store.stats.doc}} · 共 {{store.stats.total}}</span></div>

    <h4>开机自启</h4>
    <div class="row">
      <label class="switch-row">
        <input type="checkbox" :checked="store.settings.autostart === true" :disabled="store.settings.autostartBusy || store.settings.autostart === null" @change="toggleAutostart()" />
        <span>登录系统后自动启动 SnipNav（最小化到托盘）</span>
      </label>
    </div>

    <h4>AI 兜底（默认关闭；搜索无结果时让大模型生成，OpenAI 兼容接口）</h4>
    <div class="row">
      <label class="switch-row">
        <input type="checkbox" :checked="store.settings.llm.enabled" @change="toggleLlm()" />
        <span>启用 AI 兜底{{ store.settings.llm.enabled ? '' : '（当前关闭，不启用则完全不调用）' }}</span>
      </label>
    </div>
    <div class="form-grid">
      <label>API Base URL<input v-model="store.settings.llm.baseUrl" placeholder="https://open.bigmodel.cn/api/paas/v4" /></label>
      <label>模型<input v-model="store.settings.llm.model" placeholder="glm-4-flash" /></label>
    </div>
    <label>API Key<input v-model="store.settings.llm.apiKey" type="password" placeholder="sk-..." /></label>
    <div class="row end">
      <span class="small muted">{{store.settings.llmSaved}}</span>
      <button class="btn primary" @click="saveLlm()">保存 AI 配置</button>
    </div>

    <h4>导入 tldr 速查页</h4>
    <p class="muted small">先 clone 任一 tldr 格式仓库（官方 tldr-pages/tldr 或社区中文翻译版），再把它的 pages 目录路径填进来。导入是增量的，重复执行会自动跳过已有条目。</p>
    <div class="row">
      <input v-model="store.settings.importPath" placeholder="例：C:\\repo\\tldr\\pages\\common" @keydown.enter="doImport" />
      <button class="btn primary" :disabled="store.settings.importing" @click="doImport">{{store.settings.importing ? '导入中…' : '导入'}}</button>
    </div>
    <p v-if="store.settings.message" class="small">{{store.settings.message}}</p>
    <div class="row end"><button class="btn" @click="store.settings.open = false">关闭</button></div>
  </div>
</div>
`;

const DOC_TPL = `
<div class="overlay" v-if="store.docBody.open" @click.self="store.docBody.open = false">
  <div class="modal doc-modal">
    <h3>📖 {{store.docBody.title}}</h3>
    <div v-if="store.docBody.loading" class="muted pad">读取中…</div>
    <pre v-else class="doc-body">{{store.docBody.text}}</pre>
    <div class="row end"><button class="btn" @click="store.docBody.open = false">关闭</button></div>
  </div>
</div>
`;

const HISTORY_TPL = `<div class="overlay" v-if="store.historyModal.open" @click.self="store.historyModal.open = false">
  <div class="modal">
    <h3>🕘 最近执行的命令</h3>
    <p class="muted small">来自 shell 钩子记录（snip hook --install 安装）。点「存为记录」补一句人话描述即可入库；已入库的会标 ✅。</p>
    <div v-if="store.historyModal.loading" class="muted pad">读取中…</div>
    <div v-else-if="!store.historyModal.list.length" class="muted pad">暂无历史记录。终端执行：<code>snip hook --install</code>，重开终端后开始记录。</div>
    <template v-else>
      <div class="hist-list">
        <div class="hist-row" v-for="(e, i) in store.historyModal.list" :key="i">
          <div class="hist-main">
            <code>{{e.cmd}}</code>
            <span class="muted small">{{e.ts}}</span>
          </div>
          <span v-if="e.saved" class="muted small">✅ 已入库</span>
          <button v-else class="btn sm" @click="adoptFromHistory(e)">＋ 存为记录</button>
        </div>
      </div>
    </template>
    <div class="row end"><button class="btn" @click="store.historyModal.open = false">关闭</button></div>
  </div>
</div>
`;

// ---------- 组件共享 setup ----------
// 生产版 Vue 编译模板会把标识符解析为组件实例属性（_ctx.xxx），
// 全局作用域的 store/函数必须显式暴露，这里统一给所有组件用。
function sharedSetup() {
  return {
    store,
    SNIP,
    doSearch,
    setProject,
    openCreate,
    openEdit,
    closeDialog,
    saveDialog,
    removeSnippet,
    copyResult,
    copyComposed,
    saveComposed,
    select,
    closePanel,
    isActive,
    togglePart,
    toggleSuffix,
    shortCmd,
    doImport,
    toggleTheme,
    genParams,
    openSettings,
    saveLlm,
    toggleLlm,
    toggleAutostart,
    openHistory,
    adoptFromHistory,
    refreshHistory,
    openDocBody,
    aiSuggest,
  };
}

// ---------- 启动 ----------
createApp({
  setup: sharedSetup,
  components: {
    'snippet-card': { props: { r: { type: Object, required: true } }, setup: sharedSetup, template: CARD_TPL },
    'explore-panel': { setup: sharedSetup, template: PANEL_TPL },
    'record-dialog': { setup: sharedSetup, template: DIALOG_TPL },
    'settings-modal': { setup: sharedSetup, template: SETTINGS_TPL },
    'history-modal': { setup: sharedSetup, template: HISTORY_TPL },
    'doc-body-modal': { setup: sharedSetup, template: DOC_TPL },
  },
  template: `
<div class="app">
  <header class="topbar">
    <div class="brand"><span class="logo">🧭</span><b>SnipNav</b><span class="ver">v{{store.appVersion || '?'}} 本地命令速查</span></div>
    <div class="top-actions">
      <button class="btn" :title="store.theme === 'dark' ? '切换到明亮模式' : '切换到暗黑模式'" @click="toggleTheme()">{{ store.theme === 'dark' ? '🌙' : '☀️' }}</button>
      <button class="btn" title="最近执行的命令" @click="openHistory()">🕘 历史</button>
      <button class="btn" @click="openSettings()">⚙️ 设置</button>
      <button class="btn primary" @click="openCreate()">＋ 新记录</button>
    </div>
  </header>

  <div class="searchbar">
    <div class="search-row">
      <span class="s-icon">🔍</span>
      <input v-model="store.q" placeholder="搜命令 / 场景 / 标签，比如：docker 日志 过滤" @keydown.esc="store.q = ''" />
      <span class="s-hint">/ 聚焦</span>
    </div>
    <div class="chips">
      <span class="chips-label">项目</span>
      <button class="chip" :class="{ on: !store.project }" @click="setProject('')">全部</button>
      <button class="chip" v-for="p in store.stats.projects" :key="'pj-' + p" :class="{ on: store.project === p }" :title="p" @click="setProject(p)">📁 {{p}}</button>
      <span class="chips-gap"></span>
      <span class="chips-label">常用</span>
      <button class="chip" v-for="t in store.quickTags" :key="'tg-' + t" :class="{ on: store.q === t }" @click="store.q = t">{{t}}</button>
    </div>
  </div>

  <main class="layout" :class="{ withPanel: !!store.panel }">
    <section class="results">
      <div v-if="store.searching" class="muted pad">搜索中…</div>
      <div v-else-if="!store.results.length" class="empty">
        <div>没有匹配的记录</div>
        <div class="muted small">换个关键词试试，或点右上角「＋ 新记录」存一条</div>
        <button v-if="store.llmEnabled && store.q.trim()" class="btn" :disabled="store.aiLoading" style="margin-top: 12px;" @click="aiSuggest()">
          {{ store.aiLoading ? 'AI 生成中…' : '🤖 让 AI 生成几条试试' }}
        </button>
      </div>
      <template v-else>
        <snippet-card v-for="r in store.results" :key="r.id" :r="r"></snippet-card>
        <div v-if="store.results.length >= 50" class="muted small pad">只显示前 50 条，换个更具体的关键词</div>
      </template>
    </section>
    <aside v-if="store.panel" class="panel">
      <explore-panel></explore-panel>
    </aside>
  </main>

  <footer class="statusbar">
    <span>共 {{store.stats.total}} 条</span><span class="sep"></span>
    <span>🟢 个人 {{store.stats.self}}</span><span class="sep"></span>
    <span>📖 tldr {{store.stats.tldr}}</span><span class="sep"></span>
    <span>📄 文档 {{store.stats.doc}}</span>
    <span class="dir">{{store.stats.dataDir}}</span>
  </footer>

  <record-dialog></record-dialog>
  <settings-modal></settings-modal>
  <history-modal></history-modal>
  <doc-body-modal></doc-body-modal>
  <div class="toast" v-if="store.toast">{{store.toast}}</div>
</div>
`,
  mounted() {
    doSearch();
    refreshStats();
    refreshTags();
    refreshLlmFlag();
  },
}).mount('#app');
