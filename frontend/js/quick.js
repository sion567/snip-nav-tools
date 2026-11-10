// SnipNav 速查浮窗（v1.5，全局快捷键呼出）
// 交互对齐 Raycast/uTools：呼出即搜 → ↑↓ 选择 → Enter 复制 → 窗口自动隐藏
// 独立于主界面 app.js，仅复用 api.js 的 SNIP.invoke 与主题变量。

const { createApp, reactive, nextTick } = Vue;

const store = reactive({
  q: '',
  results: [],
  active: 0,
  searching: false,
  copiedId: null,
  pasteMode: false,   // v1.8：Enter 复制并粘贴回原窗口（设置里开启）
});

async function loadSettings() {
  try {
    const s = await SNIP.invoke('get_settings');
    store.pasteMode = s.quick_paste === '1';
  } catch (e) { /* 忽略 */ }
}

// ---------- 搜索 ----------
let searchTimer = null;
function scheduleSearch() {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(doSearch, 160);
}
async function doSearch() {
  store.searching = true;
  try {
    store.results = await SNIP.invoke('search', { q: store.q, project: null });
    if (store.active >= store.results.length) store.active = 0;
  } catch (e) { /* 浮窗内静默失败 */ }
  finally { store.searching = false; }
}

function move(d) {
  if (!store.results.length) return;
  store.active = (store.active + d + store.results.length) % store.results.length;
  nextTick(() => {
    const el = document.querySelector('.qr.active');
    if (el) el.scrollIntoView({ block: 'nearest' });
  });
}

// ---------- 复制即走 ----------
async function pick(r) {
  const text = SNIP.expandAutoVars(r.cmd);
  // v1.8 粘贴模式：写剪贴板 + 藏浮窗 + 向呼出前的窗口模拟 Ctrl+V
  if (store.pasteMode) {
    try {
      await SNIP.invoke('paste_and_hide', { text });
      try { await SNIP.invoke('copy_snippet', { id: r.id }); } catch (e2) { /* 忽略 */ }
      return;
    } catch (e) { /* 失败落回普通复制 */ }
  }
  try {
    await SNIP.invoke('copy_to_clipboard', { text });
  } catch (e) {
    return; // 复制失败不隐藏，让用户看到没反应
  }
  try { await SNIP.invoke('copy_snippet', { id: r.id }); } catch (e) { /* 使用量回写失败不影响 */ }
  store.copiedId = r.id;
  setTimeout(() => { SNIP.invoke('quick_hide').catch(() => {}); }, 260);
}

function srcMark(s) {
  return s === 'self' ? '🟢' : s === 'tldr' ? '📖' : s === 'ai' ? '🤖' : '📄';
}

// ---------- 键盘 ----------
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    SNIP.invoke('quick_hide').catch(() => {});
  } else if (e.key === 'ArrowDown') {
    move(1); e.preventDefault();
  } else if (e.key === 'ArrowUp') {
    move(-1); e.preventDefault();
  } else if (e.key === 'Enter') {
    const r = store.results[store.active];
    if (r) pick(r);
  }
});

// 每次快捷键呼出窗口都会重新获得焦点：刷新结果 + 设置 + 全选搜索词（打字即覆盖）
window.addEventListener('focus', () => {
  store.copiedId = null;
  const input = document.querySelector('.q-search input');
  if (input) { input.focus(); input.select(); }
  doSearch();
  loadSettings();
});

const TPL = `
<div class="q-wrap">
  <div class="q-search">
    <span class="s-icon">🔍</span>
    <input v-model="store.q" @input="scheduleSearch"
           placeholder="搜命令 / 场景 / 标签…" />
    <span class="q-hint">{{ store.pasteMode ? '↑↓ 选择 · Enter 复制并粘贴 · Esc 关闭' : '↑↓ 选择 · Enter 复制 · Esc 关闭' }}</span>
  </div>
  <div class="q-results">
    <div v-if="store.searching && !store.results.length" class="muted pad">搜索中…</div>
    <div v-else-if="!store.results.length" class="muted pad">
      没有匹配的记录——右键托盘 🧭 打开主窗口添加，或在设置里导入 tldr
    </div>
    <template v-else>
      <div v-for="(r, i) in store.results" :key="r.id"
           class="qr" :class="{ active: i === store.active, copied: store.copiedId === r.id }"
           @mouseenter="store.active = i" @click="pick(r)">
        <div class="qr-top">
          <span class="q-mark">{{ srcMark(r.source) }}</span>
          <span class="q-intent">{{ r.intent }}</span>
          <span class="q-warn" v-if="r.danger" :title="'危险操作：' + r.danger">⚠️</span>
          <span class="muted small" v-if="r.project">📁 {{ r.project }}</span>
          <span class="q-copied" v-if="store.copiedId === r.id">✅ 已复制</span>
        </div>
        <pre class="cmd">{{ r.cmd }}</pre>
      </div>
      <div v-if="store.results.length >= 50" class="muted small pad q-more">
        已显示 50 条——输入更具体的关键词可收窄
      </div>
    </template>
  </div>
</div>
`;

createApp({
  setup() {
    return { store, scheduleSearch, pick, srcMark };
  },
  template: TPL,
}).mount('#app');

// 启动：首屏加载结果 + 聚焦搜索框（之后的每次快捷键呼出由 focus 事件接手：
// 重新搜索 + 全选已有词，打字即覆盖）
doSearch();
loadSettings();
{
  const input = document.querySelector('.q-search input');
  if (input) input.focus();
}
