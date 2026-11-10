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
              wfPath: '', wfImporting: false, wfMessage: '',
              llm: { enabled: false, baseUrl: '', apiKey: '', model: '' }, llmSaved: '', autostart: null, autostartBusy: false,
              ollamaMsg: '', ollamaModels: [],
              hotkey: '', hotkeySaved: '', clipWatch: false, quickPaste: false },
  llmEnabled: false,            // AI 兜底总开关（默认关闭，设置里开启）
  historyModal: { open: false, list: [], loading: false },
  aiLoading: false,
  docBody: { open: false, title: '', text: '', loading: false },
  docBrowse: { open: false, q: '', loading: false, data: null },  // v1.4 文档库浏览
  ph: [],                       // v1.5 占位符填空：[{token, name, value}]（随选中记录重置）
  pendingClip: null,            // v1.6 剪贴板待入库命令 {cmd, ts}（监听线程写入，轮询取出）
  pb: { open: false, list: [], loading: false, editing: false,
        form: { id: null, name: '', note: '', steps: [] },
        picker: { q: '', results: [] } },  // v1.7 Playbook 运行手册
  heat: { open: false, loading: false, days: [], grid: [], total: 0 },  // v1.9 使用热力图
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
  // v1.5：基础命令先过占位符填空，再拼参数
  const raw = store.panel ? store.panel.base.cmd : '';
  const base = fillPlaceholders(raw, store.ph);
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

// v1.5 收藏置顶
async function togglePin(r) {
  try {
    const upd = await SNIP.invoke('set_pinned', { id: r.id, pinned: !r.pinned });
    Object.assign(r, upd);
    showToast(r.pinned ? '📌 已置顶，搜索恒排最前' : '已取消置顶');
  } catch (e) {
    showToast('操作失败：' + e);
  }
}

async function copyResult(r) {
  // v1.7：复制前展开动态变量（{{date}} 等），用户占位符保持原样
  if (!(await copyText(SNIP.expandAutoVars(r.cmd)))) return showToast('复制失败');
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
  store.ph = parsePlaceholders(r.cmd);
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
  // v1.5：占位符没填完就拦下——复制一条带 {{}} 的命令出去大概率跑错
  const left = unfilledCount(store.ph);
  if (left > 0) return showToast('还有 ' + left + ' 个占位符未填，填好再复制');
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

// ---------- v1.5 参数占位符填空（navi/Warp 式） ----------
// 识别 {{名称}}（tldr 风格）与 <名称>（navi 风格）。<名称> 内部不允许空白，
// 避免误伤 shell 的输入重定向（sort <in >out）与 2>/dev/null 这类语法。
function parsePlaceholders(cmd) {
  const out = [];
  const seen = new Set();
  const re = /\{\{\s*([^{}\s][^{}]*?)\s*\}\}|<([^\s<>]+)>/g;
  let m;
  while ((m = re.exec(cmd || ''))) {
    const name = (m[1] || m[2]).trim();
    if (!name || seen.has(m[0])) continue;
    seen.add(m[0]);
    // v1.7：date/time/year 是动态变量，复制时自动展开，不算「未填」
    const auto = SNIP.autoVarNames.includes(name.toLowerCase()) ? name.toLowerCase() : null;
    out.push({ token: m[0], name, value: '', auto });
  }
  return out;
}

function fillPlaceholders(cmd, phs) {
  let out = cmd || '';
  for (const p of phs) {
    const v = (p.value || '').trim() || (p.auto ? SNIP.autoVarValue(p.auto) : null);
    if (v) out = out.split(p.token).join(v);
  }
  return out;
}

function unfilledCount(phs) {
  return phs.filter((p) => !p.auto && !(p.value || '').trim()).length;
}

function hasPh(cmd) {
  return parsePlaceholders(cmd).length > 0;
}

// v1.6 危险命令（Mock 模式用，与 Rust tagging::danger_reason 同款规则的子集）
function dangerOf(cmd) {
  const c = (cmd || '').toLowerCase();
  const rules = [
    ['rm -rf', '递归强删，不可恢复'], ['git reset --hard', '丢弃全部未提交修改'],
    ['git push --force', '强制覆盖远端历史'], ['git clean -f', '删除未跟踪文件'],
    ['drop table', '删表'], ['drop database', '删库'],
    ['docker system prune -a', '删除全部未使用镜像'], ['docker system prune', '清理未用容器/网络/缓存'],
    ['kubectl delete', '删除 K8s 资源'], ['chmod 777', '全开放权限'],
  ];
  for (const [p, w] of rules) if (c.includes(p)) return w;
  return null;
}

// 填空后的基础命令（面板头部预览与组合命令共用）
store.displayBaseCmd = computed(() =>
  store.panel ? fillPlaceholders(store.panel.base.cmd, store.ph) : ''
);

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
    // 从剪贴板横幅存进来的：入库成功即清掉待入库状态
    if (store.pendingClip && f.cmd.trim() === store.pendingClip.cmd) dismissPending();
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
  try {
    const s2 = await SNIP.invoke('get_settings');
    store.settings.hotkey = s2.quick_hotkey !== undefined ? s2.quick_hotkey : 'Alt+Space';
    store.settings.clipWatch = s2.clipboard_watch === '1';
    store.settings.quickPaste = s2.quick_paste === '1';
  } catch (e) { store.settings.hotkey = 'Alt+Space'; }
  try { store.settings.autostart = await SNIP.invoke('autostart_status'); } catch (e) { store.settings.autostart = null; }
}

// v1.6：剪贴板监听开关（线程每轮读设置，即时生效，无需重启）
async function toggleClipWatch() {
  const next = !(store.settings.clipWatch === true);
  try {
    await SNIP.invoke('save_settings', { values: { clipboard_watch: next ? '1' : '0' } });
    store.settings.clipWatch = next;
    showToast(next ? '剪贴板监听已开启，复制命令后这里会提醒' : '剪贴板监听已关闭');
  } catch (e) {
    showToast('切换失败：' + e);
  }
}

// v1.5：修改速查浮窗全局快捷键（空 = 禁用）
async function saveHotkey() {
  store.settings.hotkeySaved = '';
  try {
    await SNIP.invoke('set_quick_hotkey', { hotkey: store.settings.hotkey });
    store.settings.hotkeySaved = '✅ 已生效';
  } catch (e) {
    store.settings.hotkeySaved = '❌ ' + e;
  }
  setTimeout(() => { store.settings.hotkeySaved = ''; }, 2500);
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

// v1.10：探测本地 Ollama——自动填地址、清 Key、拉模型列表
async function detectOllama() {
  store.settings.ollamaMsg = '';
  store.settings.llm.baseUrl = 'http://localhost:11434/v1';
  store.settings.llm.apiKey = '';
  try {
    const models = await SNIP.invoke('llm_models', { base_url: store.settings.llm.baseUrl });
    store.settings.ollamaModels = models;
    if (!store.settings.llm.model && models.length) store.settings.llm.model = models[0];
    store.settings.ollamaMsg = '✅ 检测到 ' + models.length + ' 个本地模型，挑一个后记得「保存 AI 配置」';
  } catch (e) {
    store.settings.ollamaModels = [];
    store.settings.ollamaMsg = '❌ ' + e;
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

// ---------- v1.4：文档库浏览（全源、分组、不截断；空查询=工具索引） ----------
let docBrowseTimer = null;
async function refreshDocBrowse() {
  store.docBrowse.loading = true;
  try {
    store.docBrowse.data = await SNIP.invoke('browse_docs', { q: store.docBrowse.q });
  } catch (e) {
    showToast('文档库读取失败：' + e);
  } finally {
    store.docBrowse.loading = false;
  }
}
function openDocBrowse(q) {
  store.docBrowse.q = String(q !== undefined ? q : store.q).trim();
  store.docBrowse.open = true;
  refreshDocBrowse();
}
function closeDocBrowse() { store.docBrowse.open = false; }
// 弹窗内搜索框防抖（开着的弹窗才响应输入）
watch(() => store.docBrowse.q, () => {
  if (!store.docBrowse.open) return;
  clearTimeout(docBrowseTimer);
  docBrowseTimer = setTimeout(refreshDocBrowse, 300);
});

async function refreshHistory() {
  try { store.historyModal.list = await SNIP.invoke('recent_history', { limit: 100 }); } catch (e2) { /* 忽略 */ }
}

// ---------- v1.6：剪贴板监听 ----------
async function refreshPending() {
  try { store.pendingClip = await SNIP.invoke('pending_clipboard'); } catch (e) { /* 忽略 */ }
}
function dismissPending() {
  store.pendingClip = null;
  SNIP.invoke('dismiss_pending_clipboard').catch(() => {});
}
function adoptPending() {
  const p = store.pendingClip;
  if (!p) return;
  openDialog({ intent: '', cmd: p.cmd, tags: '', project: '', notes: '', variantOf: null });
}
// 轮询（4s）+ 窗口聚焦时立即查：监听线程在后台写，主窗口亮着就能看到横幅
setInterval(refreshPending, 4000);
window.addEventListener('focus', refreshPending);

// ---------- v1.7：Playbook 运行手册 ----------
let pbPickerTimer = null;
async function openPlaybooks() {
  store.pb.open = true;
  store.pb.editing = false;
  store.pb.loading = true;
  try {
    store.pb.list = await SNIP.invoke('playbooks');
  } catch (e) {
    showToast('手册读取失败：' + e);
  } finally {
    store.pb.loading = false;
  }
}
function closePlaybooks() { store.pb.open = false; }

function pbStartNew() {
  store.pb.editing = true;
  store.pb.form = { id: null, name: '', note: '', steps: [] };
  store.pb.picker = { q: '', results: [] };
}
function pbEdit(p) {
  store.pb.editing = true;
  store.pb.form = { id: p.id, name: p.name, note: p.note || '', steps: p.steps.slice() };
  store.pb.picker = { q: '', results: [] };
}
function pbCancelEdit() { store.pb.editing = false; }

// 编辑器里的选段搜索（防抖，复用 search 权重排序）
function pbPickSearch() {
  clearTimeout(pbPickerTimer);
  pbPickerTimer = setTimeout(async () => {
    try {
      store.pb.picker.results = await SNIP.invoke('search', { q: store.pb.picker.q, project: null });
    } catch (e) { /* 忽略 */ }
  }, 250);
}
function pbAddStep(s) {
  const f = store.pb.form;
  if (f.steps.some((x) => x.id === s.id)) return;
  f.steps.push(s);
}
function pbRemoveStep(i) { store.pb.form.steps.splice(i, 1); }
function pbMoveStep(i, d) {
  const steps = store.pb.form.steps;
  const j = i + d;
  if (j < 0 || j >= steps.length) return;
  const [x] = steps.splice(i, 1);
  steps.splice(j, 0, x);
}

async function pbSave() {
  const f = store.pb.form;
  if (!f.name.trim()) return showToast('手册名称不能为空');
  if (!f.steps.length) return showToast('至少加一个步骤');
  try {
    const saved = await SNIP.invoke('playbook_save', {
      payload: { id: f.id, name: f.name.trim(), note: f.note.trim() || null, stepIds: f.steps.map((s) => s.id) },
    });
    store.pb.editing = false;
    showToast('手册已保存：' + saved.name);
    openPlaybooks();
  } catch (e) {
    showToast('保存失败：' + e);
  }
}

async function pbDelete(p) {
  if (!window.confirm('删除手册「' + p.name + '」？\n（只删手册组织，不删里面的命令记录）')) return;
  try {
    await SNIP.invoke('playbook_delete', { id: p.id });
    showToast('已删除手册');
    openPlaybooks();
  } catch (e) {
    showToast('删除失败：' + e);
  }
}

// 复制整本：逐行展开动态变量后拼接；每步记一次使用（跑手册就是真用了）
async function pbCopyAll(p) {
  const text = p.steps.map((s) => SNIP.expandAutoVars(s.cmd)).join('\n');
  if (!(await copyText(text))) return showToast('复制失败');
  p.steps.forEach((s) => { SNIP.invoke('copy_snippet', { id: s.id }).catch(() => {}); });
  showToast('已复制 ' + p.steps.length + ' 步');
}

// v1.8：浮窗粘贴模式开关（默认关）
async function toggleQuickPaste() {
  const next = !(store.settings.quickPaste === true);
  try {
    await SNIP.invoke('save_settings', { values: { quick_paste: next ? '1' : '0' } });
    store.settings.quickPaste = next;
    showToast(next ? '浮窗粘贴已开启：Enter 复制并粘贴回原窗口' : '浮窗粘贴已关闭');
  } catch (e) {
    showToast('切换失败：' + e);
  }
}

// ---------- v1.9：使用热力图（GitHub 式年度格子） ----------
async function openStats() {
  store.heat.open = true;
  store.heat.loading = true;
  try {
    store.heat.days = await SNIP.invoke('heatmap');
  } catch (e) {
    showToast('热力图读取失败：' + e);
  } finally {
    store.heat.loading = false;
  }
  store.heat.grid = buildHeatGrid(store.heat.days);
  store.heat.total = store.heat.days.reduce((s, d) => s + d.count, 0);
}
function closeStats() { store.heat.open = false; }

// 稀疏 [{date, count}] → 周列网格（列=周，行=周日..周六），level 0-4 按次数分档
function buildHeatGrid(days) {
  const map = {};
  let max = 0;
  for (const d of days || []) { map[d.date] = d.count; if (d.count > max) max = d.count; }
  const today = new Date();
  today.setHours(12, 0, 0, 0); // 避免时区/夏令时边界
  const start = new Date(today);
  start.setDate(start.getDate() - 364);
  start.setDate(start.getDate() - start.getDay()); // 回到当周的周日
  const levelOf = (n) => (n <= 0 ? 0 : Math.min(4, 1 + Math.floor((n - 1) / Math.max(1, Math.ceil(max / 4)))));
  const weeks = [];
  const cur = new Date(start);
  while (cur <= today) {
    const cells = [];
    let label = '';
    for (let i = 0; i < 7; i++) {
      const iso = cur.getFullYear() + '-' + String(cur.getMonth() + 1).padStart(2, '0') + '-' + String(cur.getDate()).padStart(2, '0');
      const inRange = cur <= today;
      const n = map[iso] || 0;
      cells.push({ date: iso, count: inRange ? n : -1, level: inRange ? levelOf(n) : 0 });
      cur.setDate(cur.getDate() + 1);
    }
    // 该周含某月 1 号 → 挂月份标签（GitHub 式）
    const first = cells.find((c) => c.count >= 0 && c.date.slice(8, 10) === '01');
    if (first) label = Number(first.date.slice(5, 7)) + '月';
    weeks.push({ label, cells });
  }
  return weeks;
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

// v1.9：工作流导入（Warp / Espanso / VS Code，自动识别格式）
async function doImportWorkflows() {
  const p = store.settings.wfPath.trim();
  if (!p) return;
  store.settings.wfImporting = true;
  store.settings.wfMessage = '';
  try {
    const r = await SNIP.invoke('import_workflows', { path: p });
    store.settings.wfMessage = '✅ 导入完成：' + r.files + ' 个文件，新增 ' + r.snippets + ' 条，跳过重复 ' + r.skipped + ' 条';
    doSearch();
    refreshStats();
    refreshTags();
  } catch (e) {
    store.settings.wfMessage = '❌ ' + e;
  } finally {
    store.settings.wfImporting = false;
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
    else if (store.docBrowse.open) closeDocBrowse();
    else if (store.panel) closePanel();
  }
});

// ---------- 组件模板 ----------
const CARD_TPL = `
<div class="card" :class="{ selected: store.selected && store.selected.id === r.id, pinned: r.pinned }">
  <div class="card-top">
    <span class="badge" :class="r.source">
      <template v-if="r.source === 'self'">🟢 你用过 {{r.usedCount}} 次</template>
      <template v-else-if="r.source === 'tldr'">📖 tldr 中文</template>
      <template v-else-if="r.source === 'ai'">🤖 AI 生成</template>
      <template v-else>📄 官方文档</template>
    </span>
    <span class="badge pinned" v-if="r.pinned" title="已收藏置顶，搜索恒排最前">📌 置顶</span>
    <span class="badge danger" v-if="r.danger" :title="'危险操作，执行前请确认：' + r.danger">⚠️ {{r.danger}}</span>
    <span class="proj" v-if="r.project" title="按此项目过滤" @click="setProject(r.project)">📁 {{r.project}}</span>
    <span class="badge ph" v-if="hasPh(r.cmd)" title="命令含 {{}} / <> 占位符，点「展开变体」可填空" @click="select(r)">✏️ 填空</span>
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
    <button class="btn sm" :class="{ on: r.pinned }" :title="r.pinned ? '取消置顶' : '收藏置顶（搜索恒排最前）'" @click="togglePin(r)">{{ r.pinned ? '📍' : '📌' }}</button>
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
    <pre class="cmd">{{store.displayBaseCmd}}</pre>
    <div class="row" style="justify-content: space-between; align-items: center;">
      <span class="muted small tip">点击参数 / 组合拼进「当前组合」</span>
      <button v-if="store.panel.base.tool" class="btn sm ghost" title="运行 工具 --help 解析参数，自动补进参数树（增量，可重复）" @click="genParams">⚡ --help 补充参数</button>
    </div>
  </div>

  <div class="composer ph-composer" v-if="store.ph.length">
    <div class="muted small">✏️ 占位符填空（填好后再复制，命令会实时替换）</div>
    <div class="ph-row" v-for="(p, i) in store.ph" :key="'ph-' + i">
      <code class="ph-token">{{p.token}}</code>
      <input v-if="!p.auto" v-model="p.value" :placeholder="p.name" spellcheck="false" />
      <span v-else class="ph-auto" :title="'动态变量，复制时自动替换为 ' + SNIP.autoVarValue(p.auto)">
        → {{SNIP.autoVarValue(p.auto)}} <i class="muted small">自动</i>
      </span>
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

  <div class="group" v-if="store.panel.explain && store.panel.explain.length">
    <h4>🔍 逐段解释 <span class="muted small">（绿虚线 = 参数树有说明，悬停查看；黄虚线 = 没收录，点 ⚡ --help 补充）</span></h4>
    <div class="ex-seg" v-for="(seg, si) in store.panel.explain" :key="'ex-' + si">
      <span class="ex-pipe" v-if="si > 0">|</span>
      <span v-for="(p, pi) in seg.parts" :key="'exp-' + si + '-' + pi"
            class="ex-part"
            :class="{ known: !!p.desc, unknown: !p.desc && p.text.startsWith('-') }"
            :title="p.desc || (p.text.startsWith('-') ? '参数树里还没有这个参数的说明，可点 ⚡ --help 补充' : '')">{{p.text}}</span>
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

    <h4>速查浮窗</h4>
    <p class="muted small">全局快捷键随时呼出小窗：打字即搜，↑↓ 选择，Enter 复制后自动关闭。留空 = 禁用快捷键（仍可从托盘菜单 ⚡ 打开）。</p>
    <div class="row">
      <input v-model="store.settings.hotkey" placeholder="Alt+Space（示例：Ctrl+Shift+P）" @keydown.enter="saveHotkey" spellcheck="false" />
      <button class="btn primary" @click="saveHotkey">应用</button>
      <span class="small muted">{{store.settings.hotkeySaved}}</span>
    </div>
    <div class="row" style="margin-top: 8px;">
      <label class="switch-row">
        <input type="checkbox" :checked="store.settings.quickPaste === true" @change="toggleQuickPaste()" />
        <span>浮窗 Enter 复制后自动粘贴回呼出前的窗口（默认关；模拟一次 Ctrl+V，命令仍由你在终端里自己按回车执行）</span>
      </label>
    </div>

    <h4>剪贴板监听（默认关闭）</h4>
    <div class="row">
      <label class="switch-row">
        <input type="checkbox" :checked="store.settings.clipWatch === true" @change="toggleClipWatch()" />
        <span>监听剪贴板：复制到像命令的内容时，在窗口顶部提醒一键入库（完全本地，只读文本不联网，存不存由你决定）</span>
      </label>
    </div>

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
      <label>模型<input v-model="store.settings.llm.model" list="llm-model-list" placeholder="glm-4-flash" /></label>
    </div>
    <datalist id="llm-model-list">
      <option v-for="m in store.settings.ollamaModels" :key="'om-' + m" :value="m"></option>
    </datalist>
    <label>API Key<input v-model="store.settings.llm.apiKey" type="password" placeholder="sk-...（本地 Ollama 留空）" /></label>
    <div class="row">
      <button class="btn sm" title="自动填入本地 Ollama 地址并拉取模型列表（需已 ollama serve）" @click="detectOllama">🔍 检测本地 Ollama（免 Key）</button>
      <span class="small muted">{{store.settings.ollamaMsg}}</span>
    </div>
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

    <h4>导入工作流（Warp / Espanso / VS Code）</h4>
    <p class="muted small">填文件或目录路径，格式自动识别：.json → VS Code snippets；.yaml 顶层 matches → Espanso 包；带 command 的 yaml → Warp Workflow（~/.warp/workflows）。Warp/Espanso 的 <code v-text="'{{变量}}'"></code> 会接上占位符填空，VS Code 的 $1/&#36;{2:默认} 转成 &lt;1&gt;/&lt;默认&gt;。</p>
    <div class="row">
      <input v-model="store.settings.wfPath" placeholder="例：C:\\Users\\你\\.warp\\workflows" @keydown.enter="doImportWorkflows" spellcheck="false" />
      <button class="btn primary" :disabled="store.settings.wfImporting" @click="doImportWorkflows">{{store.settings.wfImporting ? '导入中…' : '导入'}}</button>
    </div>
    <p v-if="store.settings.wfMessage" class="small">{{store.settings.wfMessage}}</p>
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

// v1.4：文档库浏览——完整性视图（与搜索的相关性排序互补）
const DOCBROWSE_TPL = `
<div class="overlay" v-if="store.docBrowse.open" @click.self="closeDocBrowse">
  <div class="modal doc-browse-modal">
    <h3>📚 文档库</h3>
    <div class="search-row">
      <span class="s-icon">🔍</span>
      <input v-model="store.docBrowse.q" placeholder="输入工具名或关键词，如：docker（多词是收窄）" />
    </div>
    <div v-if="store.docBrowse.loading && !store.docBrowse.data" class="muted pad">读取中…</div>
    <template v-else-if="store.docBrowse.data">
      <template v-if="!store.docBrowse.data.query">
        <p class="muted small">库里的工具，点一个浏览它的全部文档：</p>
        <div class="chips" v-if="store.docBrowse.data.tools.length">
          <button class="chip" v-for="t in store.docBrowse.data.tools" :key="'tool-' + t.name" :title="t.name + ' 共 ' + t.count + ' 条'" @click="store.docBrowse.q = t.name">
            {{t.name}} <b>{{t.count}}</b>
          </button>
        </div>
        <div v-else class="muted pad">库还是空的——先存一条记录，或在设置里导入 tldr / 手册页。</div>
      </template>
      <template v-else-if="!store.docBrowse.data.groups.length">
        <div class="muted pad">没有匹配「{{store.docBrowse.data.query}}」的记录</div>
      </template>
      <template v-else>
        <div class="muted small">「{{store.docBrowse.data.query}}」共 {{store.docBrowse.data.total}} 条</div>
        <div class="doc-group" v-for="g in store.docBrowse.data.groups" :key="'dg-' + g.key">
          <h4>{{g.label}} · {{g.items.length}}</h4>
          <div class="hist-row" v-for="s in g.items" :key="'dgi-' + g.key + '-' + s.id">
            <div class="hist-main">
              <code>{{s.cmd}}</code>
              <span class="muted small">{{s.intent}}</span>
            </div>
            <button class="btn sm" title="复制命令（累计使用次数）" @click="copyResult(s)">📋</button>
            <button v-if="s.source === 'doc'" class="btn sm" title="查看整页手册" @click="openDocBody(s)">📖 全文</button>
          </div>
        </div>
        <div v-if="store.docBrowse.data.total >= 1000" class="muted small">已达浏览上限 1000 条，换个更具体的关键词</div>
      </template>
    </template>
    <div class="row end"><button class="btn" @click="closeDocBrowse">关闭</button></div>
  </div>
</div>
`;

// v1.7：Playbook 运行手册——列表 / 编辑两种形态
const PB_TPL = `
<div class="overlay" v-if="store.pb.open" @click.self="closePlaybooks">
  <div class="modal pb-modal">
    <template v-if="!store.pb.editing">
      <h3>📓 运行手册</h3>
      <p class="muted small">把一个流程的多条命令按顺序打包（部署/排障/交接），一键复制全部步骤。命令本体仍是一条条普通记录。</p>
      <div class="row">
        <button class="btn primary sm" @click="pbStartNew">＋ 新建手册</button>
      </div>
      <div v-if="store.pb.loading" class="muted pad">读取中…</div>
      <div v-else-if="!store.pb.list.length" class="muted pad">还没有手册——点「＋ 新建手册」，把搜索到的命令按顺序加进去。</div>
      <div class="pb-item" v-for="p in store.pb.list" :key="'pb-' + p.id">
        <div class="pb-head">
          <b>{{p.name}}</b>
          <span class="muted small">{{p.steps.length}} 步</span>
          <button class="btn sm" title="按顺序复制全部命令（每步记一次使用）" @click="pbCopyAll(p)">📋 复制全部</button>
          <button class="btn sm" @click="pbEdit(p)">✏️ 编辑</button>
          <button class="btn sm danger" @click="pbDelete(p)">🗑</button>
        </div>
        <div class="muted small" v-if="p.note">{{p.note}}</div>
        <div class="pb-step" v-for="(s, i) in p.steps" :key="'pbs-' + p.id + '-' + s.id">
          <span class="pb-no">{{i + 1}}</span>
          <code class="pb-cmd" :title="s.intent">{{s.cmd}}</code>
          <span class="muted small" v-if="s.danger" :title="s.danger">⚠️</span>
        </div>
      </div>
      <div class="row end"><button class="btn" @click="closePlaybooks">关闭</button></div>
    </template>

    <template v-else>
      <h3>{{store.pb.form.id ? '✏️ 编辑手册' : '＋ 新建手册'}}</h3>
      <div class="form-grid">
        <label>手册名称
          <input v-model="store.pb.form.name" placeholder="例：shop-admin 发版流程" />
        </label>
        <label>备注 <i class="muted small">（何时用/给谁看）</i>
          <input v-model="store.pb.form.note" placeholder="例：周四发版前必读" />
        </label>
      </div>
      <div class="muted small">步骤（按顺序执行）：</div>
      <div class="pb-step editing" v-for="(s, i) in store.pb.form.steps" :key="'pbf-' + s.id">
        <span class="pb-no">{{i + 1}}</span>
        <code class="pb-cmd" :title="s.intent">{{s.cmd}}</code>
        <button class="btn sm" :disabled="i === 0" title="上移" @click="pbMoveStep(i, -1)">↑</button>
        <button class="btn sm" :disabled="i === store.pb.form.steps.length - 1" title="下移" @click="pbMoveStep(i, 1)">↓</button>
        <button class="btn sm danger" title="移除" @click="pbRemoveStep(i)">✕</button>
      </div>
      <div v-if="!store.pb.form.steps.length" class="muted small pad">还没选步骤——下面搜一条点「＋ 加入」。</div>

      <div class="muted small">从库里挑步骤：</div>
      <div class="row">
        <input v-model="store.pb.picker.q" @input="pbPickSearch" placeholder="搜命令 / 场景 / 标签…" spellcheck="false" />
      </div>
      <div class="pb-pick" v-if="store.pb.picker.q.trim()">
        <div class="pb-step pick" v-for="s in store.pb.picker.results" :key="'pbk-' + s.id">
          <code class="pb-cmd" :title="s.intent">{{s.cmd}}</code>
          <button class="btn sm primary" :disabled="store.pb.form.steps.some((x) => x.id === s.id)" @click="pbAddStep(s)">＋ 加入</button>
        </div>
        <div v-if="!store.pb.picker.results.length" class="muted small pad">没有匹配的记录</div>
      </div>

      <div class="row end">
        <button class="btn" @click="pbCancelEdit">取消</button>
        <button class="btn primary" @click="pbSave">保存手册</button>
      </div>
    </template>
  </div>
</div>
`;

// v1.9：使用热力图（年度回顾）
const HEAT_TPL = `
<div class="overlay" v-if="store.heat.open" @click.self="closeStats">
  <div class="modal heat-modal">
    <h3>📊 使用回顾</h3>
    <div v-if="store.heat.loading" class="muted pad">读取中…</div>
    <template v-else>
      <p class="muted small">近一年 {{store.heat.total}} 次使用（复制 / 组合 / 存变体 / 收藏都算）——格子越绿用得越勤。</p>
      <div class="heat-scroll">
        <div class="heat-grid">
          <div class="heat-col" v-for="(w, wi) in store.heat.grid" :key="'hw-' + wi">
            <span class="heat-month-label">{{ w.label || '&nbsp;' }}</span>
            <span v-for="(c, ci) in w.cells" :key="'hc-' + wi + '-' + ci"
                  class="heat-cell" :class="'h' + c.level"
                  :title="c.count < 0 ? '' : c.date + '：' + c.count + ' 次'"></span>
          </div>
        </div>
      </div>
      <div class="heat-legend muted small">
        少 <span class="heat-cell h0"></span><span class="heat-cell h1"></span><span class="heat-cell h2"></span><span class="heat-cell h3"></span><span class="heat-cell h4"></span> 多
      </div>
      <div class="kv"><span>库内记录</span><span>共 {{store.stats.total}} 条（个人 {{store.stats.self}} · tldr {{store.stats.tldr}} · 文档 {{store.stats.doc}}）</span></div>
    </template>
    <div class="row end"><button class="btn" @click="closeStats">关闭</button></div>
  </div>
</div>
`;

const HISTORY_TPL = `<div class="overlay" v-if="store.historyModal.open" @click.self="store.historyModal.open = false">
  <div class="modal">
    <h3>🕘 最近执行的命令</h3>
    <p class="muted small">两个来源：Git Bash 钩子（<code>snip hook --install</code> 安装，带时间）与 PowerShell PSReadLine 历史（⚡ 标记，自动读取无需安装）。点「存为记录」补一句人话描述即可入库；已入库的会标 ✅。</p>
    <div v-if="store.historyModal.loading" class="muted pad">读取中…</div>
    <div v-else-if="!store.historyModal.list.length" class="muted pad">暂无历史记录。Git Bash 执行 <code>snip hook --install</code>；PowerShell 历史自动读取（Windows）。</div>
    <template v-else>
      <div class="hist-list">
        <div class="hist-row" v-for="(e, i) in store.historyModal.list" :key="i">
          <div class="hist-main">
            <code>{{e.cmd}}</code>
            <span class="muted small">{{ e.shell === 'powershell' ? '⚡ PowerShell' : e.ts }}</span>
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
    togglePin,
    copyResult,
    copyComposed,
    saveComposed,
    select,
    closePanel,
    isActive,
    togglePart,
    toggleSuffix,
    shortCmd,
    hasPh,
    doImport,
    toggleTheme,
    genParams,
    openSettings,
    saveLlm,
    toggleLlm,
    detectOllama,
    toggleAutostart,
    saveHotkey,
    toggleClipWatch,
    toggleQuickPaste,
    adoptPending,
    dismissPending,
    openPlaybooks,
    closePlaybooks,
    openStats,
    closeStats,
    doImportWorkflows,
    pbStartNew,
    pbEdit,
    pbCancelEdit,
    pbPickSearch,
    pbAddStep,
    pbRemoveStep,
    pbMoveStep,
    pbSave,
    pbDelete,
    pbCopyAll,
    openHistory,
    adoptFromHistory,
    refreshHistory,
    openDocBody,
    openDocBrowse,
    closeDocBrowse,
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
    'playbook-modal': { setup: sharedSetup, template: PB_TPL },
    'heat-modal': { setup: sharedSetup, template: HEAT_TPL },
    'doc-body-modal': { setup: sharedSetup, template: DOC_TPL },
    'doc-browse-modal': { setup: sharedSetup, template: DOCBROWSE_TPL },
  },
  template: `
<div class="app">
  <header class="topbar">
    <div class="brand"><span class="logo">🧭</span><b>SnipNav</b><span class="ver">v{{store.appVersion || '?'}} 本地命令速查</span></div>
    <div class="top-actions">
      <button class="btn" :title="store.theme === 'dark' ? '切换到明亮模式' : '切换到暗黑模式'" @click="toggleTheme()">{{ store.theme === 'dark' ? '🌙' : '☀️' }}</button>
      <button class="btn" title="最近执行的命令" @click="openHistory()">🕘 历史</button>
      <button class="btn" title="使用回顾：近一年热力图" @click="openStats()">📊 回顾</button>
      <button class="btn" title="运行手册：多条命令按顺序打包" @click="openPlaybooks()">📓 手册</button>
      <button class="btn" title="按来源浏览库里的全部文档" @click="openDocBrowse()">📚 文档库</button>
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

  <div class="clip-banner" v-if="store.pendingClip">
    <span class="cb-label">📋 剪贴板发现命令 <span class="muted small">{{store.pendingClip.ts}}</span></span>
    <code class="cb-cmd" :title="store.pendingClip.cmd">{{store.pendingClip.cmd}}</code>
    <button class="btn sm primary" @click="adoptPending">＋ 存为记录</button>
    <button class="btn sm ghost" title="不再提醒这条" @click="dismissPending">✕</button>
  </div>

  <main class="layout" :class="{ withPanel: !!store.panel }">
    <section class="results">
      <div class="browse-entry" v-if="store.q.trim()">
        <button class="chip" title="按来源分组浏览全部匹配文档（不截断）" @click="openDocBrowse(store.q)">📖 「{{store.q}}」的全部文档</button>
      </div>
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
  <playbook-modal></playbook-modal>
  <heat-modal></heat-modal>
  <doc-body-modal></doc-body-modal>
  <doc-browse-modal></doc-browse-modal>
  <div class="toast" v-if="store.toast">{{store.toast}}</div>
</div>
`,
  mounted() {
    doSearch();
    refreshStats();
    refreshTags();
    refreshLlmFlag();
    refreshPending();
  },
}).mount('#app');
