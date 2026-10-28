# SnipNav — 本地命令速查与智能发散工具

设计文档 v0.1（整理版）· 附实现决策（Rust 桌面版）

---

## 一、产品定位

**一句话**：离线本地命令知识库 + 搜索即查 + 点开即复制 + 参数发散面板。不连云、不连服务器、不跑大模型推理。

**解决什么问题**：

- 命令用过一次但没记住，下次要重新找
- **同一个项目的命令高度复用**——上周在这个项目里跑通的命令，这周忘了（核心场景，见「项目维度」）
- 网上搜得到但慢，而且不一定是你上次跑通的那条
- 知道命令但不知道还能加什么参数
- 换电脑 / 不联网时查不了 man 页
- 团队里夜班白班交接靠口头，没有沉淀

**不是什么**：

- 不是终端模拟器（不执行命令）
- 不是笔记软件（不做富文本、不写长文）
- 不是 AI 对话工具（不实时生成，只做检索 + 发散）

---

## 二、核心交互流程

```
┌─────────────────────────────────────────────────────────────┐
│  🔍 搜命令/场景/标签     [docker 日志 过滤]                  │
├─────────────────────────────────────────────────────────────┤
│  搜索结果（按权重排序）：                                    │
│                                                             │
│  1. 🟢 docker logs 按时间+级别过滤         你用过 12 次      │
│     docker logs -f mycontainer --since 2026-10-25 | grep ERROR│
│     [复制] [展开变体 ▾]                                     │
│                                                             │
│  2. 🟢 docker logs 只看末尾100行            你用过 5 次      │
│     docker logs --tail 100 -t mycontainer                  │
│     [复制] [展开变体 ▾]                                     │
│                                                             │
│  3. 📖 grep 过滤文本（来自 tldr 中文）                      │
│     grep -C 5 "ERROR" app.log                              │
│     [复制] [展开变体 ▾]                                     │
└─────────────────────────────────────────────────────────────┘
```

点击「展开变体」后，右侧出现发散面板：

```
┌── 变体面板 ──────────────────────────────────────┐
│  基于：docker logs -f mycontainer                │
│                                                 │
│  ▸ 加时间过滤                                   │
│    --since 2026-10-25T00:00:00   从指定时间开始 │
│    --since 1h                     最近 1 小时   │
│    --until ...                    到指定时间结束 │
│                                                 │
│  ▸ 加输出控制                                   │
│    --tail 100 / -t / --details …                │
│                                                 │
│  ▸ 加管道过滤                                   │
│    | grep "ERROR"     只留错误（你用过 12 次）  │
│    | jq '.level'      JSON 日志过滤             │
│                                                 │
│  ▸ 相关命令（你下一步可能要的）                 │
│    docker ps / docker exec -it sh / …           │
│                                                 │
│  点击参数 → 拼「当前组合」→ [复制] [存为新记录] │
└─────────────────────────────────────────────────┘
```

---

## 三、数据架构

### 3.1 数据源（三层）

| 层 | 内容 | 来源 | 更新方式 |
|---|---|---|---|
| 个人记录 | 你跑通的命令 + 场景 + 变体 | 手动存 /（v0.3）从 history 采集 | 随时增 |
| 中文速查 | tldr-pages 中文翻译 | 离线导入一次（脚本支持任意 tldr 格式目录） | 偶尔拉新版本 |
| 官方文档 | 工具 `--help` 输出结构化 | v0.2 抓取存本地 | 工具升级时更新 |

### 3.2 存储结构（用户数据目录 `~/.snipnav/`）

```
~/.snipnav/
├── data/
│   ├── snippets.db        # SQLite 主库（含 FTS5 索引表、doc_bodies 旁表、settings/meta）
│   ├── snippets.db-wal    # WAL 日志（连接活跃期存在，见 §十一 P0-1 的收尾策略）
│   └── snippets.db-shm    # WAL 共享内存索引
└── history.log            # （v0.3）shell hook 记录，>5MB 自动裁剪保留末尾 2000 行
```

> 环境变量 `SNIPNAV_HOME` 可覆盖数据目录（开发/测试用）。
>
> 与 v0.1 原稿的差异：① `config.yaml` 不再引入——应用设置（LLM 配置等）改存
> SQLite `settings` 键值表（见 §3.3），少一种文件格式；② tldr 中文页不在本地
> 留 clone，解析后直接入库；③ v0.2 的 `--help` 结构化结果进 `param_tree` 表
> （不是 JSON 文件）；④ 批量手册（man）整页原文进 `doc_bodies` 旁表（v1.1）。

### 3.3 数据库表

```sql
-- 主表：命令记录
CREATE TABLE snippets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool        TEXT,            -- docker / gradle / git
    command     TEXT,            -- logs / bootRun / clone
    project     TEXT,            -- 归属项目（目录名），CLI 存入时自动取当前目录
    intent      TEXT,            -- "看容器今天错误日志"（人话）
    cmd         TEXT,            -- 完整命令
    variant_of  INTEGER,         -- 变体指向主命令 id
    source      TEXT DEFAULT 'self',  -- 'self' / 'tldr' / 'doc' / 'ai'（v0.5）
    verified    BOOLEAN DEFAULT 1,
    used_count  INTEGER DEFAULT 0,
    last_used   DATE,
    created     DATE DEFAULT (date('now')),
    notes       TEXT
);

CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE snippet_tags (snippet_id INTEGER, tag_id INTEGER);

-- 用户行为（推荐用）
CREATE TABLE interactions (
    id INTEGER PRIMARY KEY, snippet_id INTEGER,
    action TEXT,                -- 'copy' / 'copy_variant' / 'save_variant' / 'ignore' / 'run'
    created DATE DEFAULT (date('now'))
);

-- 组合模式（你经常怎么拼）
CREATE TABLE patterns (
    id INTEGER PRIMARY KEY,
    base_cmd TEXT,              -- docker logs
    suffix   TEXT,              -- | grep ERROR
    used_count INTEGER DEFAULT 0,
    UNIQUE(base_cmd, suffix)    -- 实现新增：支持使用量累加（upsert）
);

-- 参数树（发散面板用）
CREATE TABLE param_tree (
    id       INTEGER PRIMARY KEY,
    tool     TEXT, command TEXT,
    category TEXT,              -- time / output / filter / pipe / run / other
    flag     TEXT,              -- --tail / --since
    desc     TEXT, example TEXT,
    sort_order INTEGER DEFAULT 0
);

-- v0.2：行数计数表（触发器维护，O(1) 判断是否切换 FTS 检索路径）
CREATE TABLE meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL DEFAULT 0);

-- v1.1：手册整页原文旁表（man 等），点「📖 全文」按 snippet_id 取，永不进搜索/FTS 索引
CREATE TABLE doc_bodies(
    snippet_id INTEGER PRIMARY KEY REFERENCES snippets(id) ON DELETE CASCADE,
    body TEXT
);

-- v0.5：应用设置（LLM 兜底配置等），替代原 config.yaml 方案
CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);
```

### 3.4 全文搜索索引

FTS5 表照设计创建并随 snippets 增删改同步。

> **实现修正**：FTS5 默认 unicode61 分词器不能切分中文（整句成一个 token）；trigram 分词器又要求关键词 ≥3 字符，「日志」这类 2 字词搜不到。MVP 数据量（<5000 条）下改用 **「LIKE 候选集 + Rust 侧权重排序」**，毫秒级返回且对中文天然友好；FTS5 表保留并持续同步，数据量上万后再切换检索路径。

---

## 四、核心功能设计

### 4.1 记录命令

- **方式 A（CLI）**：伴随二进制 `snip.exe`（当前目录名自动作为项目归属）
  `snip add "docker 日志按时间过滤错误" "docker logs -f mycontainer --since ... | grep ERROR" --tag docker,log,filter`
  `snip search "docker 日志" --project shop-admin`
- **方式 B（Shell hook）**：v0.3，自动追加 history.log，事后标记入库
- **方式 C（GUI）**：搜索框旁「＋ 新记录」，填人话描述 + 命令，标签可留空自动打

### 4.2 搜索

| 输入示例 | 匹配逻辑 |
|---|---|
| `docker 日志` | intent 模糊 + tags 命中 + cmd 模糊 |
| `过滤` | tags 含 filter + intent/cmd 含 grep/awk |
| `gradle 启动` | intent 含「启动」+ cmd 含 gradle/bootRun |
| `springboot 端口` | intent 含 springboot + cmd 含 port/server.port |

**排序权重**（Rust 侧打分）：

| 信号 | 分值 |
|---|---|
| 自己的记录（source=self） | +50 |
| 命中当前项目（选择项目过滤时） | +30 |
| used_count × 2 | +N |
| 7 天内用过 | +20 |
| 标签精确命中 | +10 / 个 |
| intent 模糊命中 | +5 / 词 |
| cmd 模糊命中 | +2 / 词 |

### 4.3 参数发散面板

触发：点击任意命令 → 右侧面板展开。

渲染逻辑：

1. 查 `param_tree WHERE tool=? AND command=?` → 按 category 分组（时间过滤 / 输出控制 / 内容过滤 / 管道过滤 / 运行控制）
2. 查 `patterns WHERE base_cmd=? ORDER BY used_count DESC` → 「你的常用组合」置顶展示
3. 查 `snippets WHERE tool=? AND command!=?` → 「相关命令」推荐
4. 点击任意参数/组合 → 拼进「当前组合」→ 一键 **复制组合** 或 **存为新记录**（variant_of 指向主命令）

### 4.4 标签自动生成

规则表（`tagging.rs`）：docker / k8s / gradle / maven / git / windows / linux / log / filter / deploy / build / run / network / permission，按命令文本与人话描述关键词命中。人工标签与自动标签合并去重，统一小写。

### 4.5 复制

- 结果卡复制 → 剪贴板 + `used_count+1`、`last_used`、interactions 记 `'copy'`
- 面板组合复制 → 同上 + `patterns` 该组合使用量 +1（为 4.2 权重积累数据）

---

## 五、技术方案

### 5.1 原方案（v0.1 稿）

| 层 | 方案 | 理由 |
|---|---|---|
| 存储 | SQLite（FTS5） | 本地单文件、零配置 |
| 后端 | Python FastAPI 或 Go Gin | localhost API |
| 前端 | Vue3 + Vite | 轻量 SPA |
| 部署 | 本地进程 / Tauri 打包（v1.0） | 不依赖外部 |

### 5.2 实现方案（Rust 桌面版，本次落地）

| 层 | 方案 | 理由 |
|---|---|---|
| 形态 | **Tauri 2 桌面应用**（单窗口） | 即原 v1.0 演进目标；单 exe、离线、开机即用 |
| 核心逻辑 | **Rust**（rusqlite bundled 内嵌 SQLite） | 搜索打分 / 参数树 / tldr 导入 / 标签全在 Rust，类型安全 |
| 前端 | Vue3 全局构建（本地 vendor 166KB）静态页 | 复用原 UI 设计；**无 Node 构建步骤**，运行/构建只要 Rust 工具链 |
| 剪贴板 | Rust 命令（arboard） | webview 内 clipboard 权限不可靠，走原生稳妥 |
| CLI | 伴随二进制 `snip.exe`（同库复用） | 终端里一行存命令，记录成本趋近零 |
| IPC | Tauri commands（`withGlobalTauri`） | 前端 `window.__TAURI__.core.invoke` 直调 Rust |

前端 api 层内置 **浏览器 Mock 模式**：未检测到 Tauri 环境时自动切换内存假数据，可直接用浏览器打开 `frontend/index.html` 调 UI。

---

## 六、MVP 范围（第一版只做这些）

| 功能 | 做不做 | 说明 |
|---|---|---|
| 搜索框 + 结果列表 | ✅ | 核心，必须 |
| 项目维度（归属/过滤/加权） | ✅ | 同项目命令优先，直击核心痛点 |
| 点击复制 | ✅ | 核心，必须 |
| 个人记录 CRUD | ✅ | 存/改/删 |
| 自动打标签 | ✅ | 规则提取 |
| 参数发散面板 | ✅ | 从参数树读，支持组合/复制/存变体 |
| tldr 中文导入 | ✅ | 任意 tldr 格式目录导入 + 内置 10 页种子（升级按版本号增量补导老库，参数树同机制） |
| 官方 help 抓取 | ❌ 延后 | v0.2 |
| 行为统计/推荐 | ❌ 延后 | 数据够了再加（埋点已就位） |
| Shell hook | ❌ 延后 | v0.3 |
| 大模型兜底 | ❌ 延后 | 最后才考虑 |

---

## 七、规模预估

| 内容 | 大小 |
|---|---|
| snippets.db（100 条个人记录 + 种子） | ~200KB |
| 内置 tldr 种子页 | ~10KB |
| 前端静态资源（含 vendor Vue） | ~250KB |
| 桌面 exe（release，实测 SnipNav.exe） | ~13MB |
| CLI 伴随 exe（release，实测 snip.exe） | ~2.5MB |
| 用户导入 tldr 全库（可选） | ~2MB |
| WAL 日志（见 §十一 P0-1 收尾策略） | 收尾后趋近 0 |

---

## 八、里程碑

| 阶段 | 内容 |
|---|---|
| M1 | Rust 核心：数据层 / 搜索打分 / 自动标签 / 发散面板 / tldr 导入 / 种子数据（含单元测试） |
| M2 | Tauri 壳：配置 / 图标 / 命令层 / CLI 伴随二进制 |
| M3 | 前端：搜索页 / 结果卡片 / 发散面板 / 新记录 / 设置 |
| M4 | 编译 + cargo test + CLI 冒烟 + 浏览器 Mock UI 验证 + GUI 启动验证 |
| M5 | README / 收尾 |

---

## 九、后续演进

| 版本 | 加什么 |
|---|---|
| v0.2 ✅ 已完成 | 官方 help 抓取 → 参数树自动生成（面板 ⚡ 按钮 / `snip gen-params`）；FTS5 检索自动切换（>2 万行且 ASCII 关键词时走 MATCH，中文/短词永远 LIKE）；搜索标签批量加载；tldr 导入包事务；`snip vacuum` |
| v0.3 ✅ 已完成 | Shell hook 自动记录（`snip hook --install` 幂等装到 ~/.bashrc，PROMPT_COMMAND 钩子写 history.log，>5MB 自动裁剪）；GUI 🕘 历史弹窗一键入库；行为统计（`snip stats` Top5 常用） |
| v0.4 ✅ 已完成（文件级） | 团队共享：`snip export / import-share`（JSON 导出个人记录；导入按命令智能合并——保留较大使用量、合并标签）；内网多用户实时共用库（服务化）延后 |
| v0.5 ✅ 已完成 | 大模型兜底 + 自动入库：**默认关闭**，设置里显式开启后才可用（`llm_enabled` 开关 + OpenAI 兼容接口配置，默认 GLM）；开启后搜索无结果一键生成 ≤3 条以 source='ai' 入库、🤖 徽标、可删 |
| v1.x ✅ 已完成 | 托盘（关窗=隐藏到托盘，托盘菜单 显示/退出）；开机自启（设置里开关，tauri-plugin-autostart）；NSIS 安装包（`tauri build`） |
| v1.2 ✅ 已完成 | 性能与存储 P0 包（§11.2）：WAL 收尾（journal_size_limit + 连接 Drop/GUI 退出双路 checkpoint）；热度索引 `idx_snippets_hot` 与项目索引；`llm_suggest` 持锁网络请求拆分（AI 生成不再阻塞其它 IPC 最长 30s）；种子存在性判断 LIMIT 1 化 |

---

## 十、设计原则（记住这四条）

1. **记录成本趋近于零**：一行命令或一次点击就存
2. **搜索靠人话**：匹配 intent，不逼你记命令名
3. **发散不猜**：参数树铺出来让你选，不猜你下一个要什么
4. **本地优先**：不连云、不依赖外部服务、数据在你自己机器上

---

## 十一、性能与存储优化空间（2026-10 评审 → v1.2 预案）

> 结论先行：**当前规模（个人几百条 + tldr 几千条）下没有任何性能问题**，本节是
> "数据长到十万行 / 语料上百 MB 之前值得做掉的事"，按改动量分级；P0 四项合计
> ~50 行代码，P1 两项等真要导入大语料再做，P2 记录在案暂不动。

### 11.1 现状盘点（已落地的优化，见附录 A 决策 11/12）

- WAL + `cache_size=-65536`（64MB 页缓存）+ `mmap_size=256MB`（零拷贝读）
- LIKE 候选 + Rust 打分；行数 >2 万且关键词全 ASCII ≥3 字符时自动切 FTS5 MATCH
- 大库双路合并：中文/短词 LIKE 只扫自有数据（self/tldr/ai），不白扫 doc 层英文语料
- external-content FTS5（不复制正文）；标签单条 `IN` 批量查询（无 N+1）
- tldr / 手册导入显式事务；doc_bodies 旁表使搜索数据量与语料体积解耦
- `meta` 计数表触发器维护（O(1) 行数判断）；`snip vacuum` 手动压缩

**本机实测（2026-10-26，种子数据 + 测试写入）**：主库 4KB，`snippets.db-wal` 却达
**2.1MB**（应用无退出 checkpoint，WAL 常驻膨胀）；SnipNav.exe 13.4MB / snip.exe 2.4MB。

### 11.2 P0：小改动（✅ v1.2 已全部落地，合计 ~60 行）

> 落地方式：① `db::open()` 改返回 `Conn` 包装（Deref 透传 Connection，Drop 时
> `wal_checkpoint(TRUNCATE)`），GUI 退出事件（`RunEvent::Exit`）里对全局连接显式
> checkpoint，打开时设 `journal_size_limit=64MB`；② 两条 `CREATE INDEX IF NOT
> EXISTS`（热度 / 项目）；③ `llm.rs` 拆 `prepare`（持锁读配置）/ `generate`（无锁
> 网络）/ `store`（持锁入库）三步，command 层两段持锁、网络段不持锁；
> ④ `db::is_empty()`（`SELECT 1 … LIMIT 1`）。已验证：30 项单测全绿；CLI 冒烟
> （临时 SNIPNAV_HOME）进程退出后 `-wal`/`-shm` 文件归零消失，索引正确建立。

| # | 优化 | 问题证据 / 收益 | 改动点 |
|---|---|---|---|
| 1 | **WAL 收尾**：连接关闭前 `PRAGMA wal_checkpoint(TRUNCATE)`；打开时设 `journal_size_limit=67108864` | 实测 4KB 主库挂 2.1MB WAL；收尾后磁盘占用回到主库真实大小 | `db.rs::open()` 加一行 pragma；GUI 退出事件 + CLI 各命令结束前 checkpoint（约 15 行） |
| 2 | **热度索引**：`CREATE INDEX idx_snippets_hot ON snippets(used_count DESC, last_used DESC, id DESC)` | 首页空查询、面板「相关命令」全局补齐都是 `ORDER BY used_count DESC …`，无索引时大库（doc 层十万行）每次启动全表扫描+排序 | `db.rs` BASE_SCHEMA 加一条 `CREATE INDEX IF NOT EXISTS`（3 行） |
| 3 | **`llm_suggest` 持锁网络请求拆分**：当前 command 全程持有全局 `Db` 互斥锁调 LLM（超时 30s），期间搜索/点卡片等所有 IPC 全部阻塞 | 用户点一次「AI 生成」，界面最长卡 30 秒 | `commands.rs` + `llm.rs`：先持锁读配置→释放→裸网络请求→再持锁入库（约 20 行重构，纯 Rust 侧，IPC 契约不变） |
| 4 | **存在性判断微优化**：`seed_all` 里 3 个 `COUNT(*)` 改 `SELECT 1 … LIMIT 1`；`stats` 的 `DISTINCT project` 复用 #2 思路加 `idx_snippets_project`（或维持现状，表小无感） | 微秒级 → 无感级，顺手做 | `seeds.rs` / `commands.rs`（约 10 行） |

### 11.3 P1：中等改动，导入大语料（100MB 级手册）前再做

| # | 优化 | 收益 | 代价 / 风险 |
|---|---|---|---|
| 5 | **doc_bodies 压缩存储**：整页原文 deflate/zstd 压缩后存 BLOB（头部加 1 字节格式版本，旧明文兼容读、懒迁移） | 手册纯文本压缩比 ~3-4×，100-200MB 语料 → 30-60MB；读取仍是按 id O(1)，多一次解压（微秒级） | 加 `flate2` 依赖；`docs.rs` 读写两处 + 迁移函数（约 60 行）；需补单测 |
| 6 | **FTS 延迟创建**：行数首次越过 `FTS_THRESHOLD` 才建 `snippets_fts` 并 `INSERT SELECT` 回填 | 小库省掉 FTS 索引体积（约为可检索文本的 20-40%）与每行写入的三个触发器开销；小库现状代价仅几十 KB，属"顺路洁癖"，配合 #5 的大库才有量级意义 | `db.rs::open()` 按行数条件建表 + 回填函数（约 40 行）；`has_fts` 判断已就位，检索切换天然兼容 |

### 11.4 P2：记录在案，暂不做

- **interactions 无限增长**：每次复制插一行，年增 ~1 万行（≈1MB 量级）。等真到大
  再加"保留最近 N 条"清理或按 (snippet_id, action) 聚合。
- **VACUUM 增量化**：`auto_vacuum=INCREMENTAL`（需建库时设定 + 一次全量 VACUUM
  切换）+ `snip vacuum` 改调 `incremental_vacuum`。当前 <10MB 库全量 VACUUM 瞬间
  完成，只有大库才需要。
- **LIKE 候选集 SQL 层封顶**（如 `ORDER BY used_count DESC LIMIT 5000` 子查询再
  打分）：防极端关键词（单字母）在大库拖慢 LIKE 路径，但牺牲长尾召回，与"搜索
  靠人话"原则有张力，默认不做。

### 11.5 顺手项（与性能无关的已知小问题，✅ 已全部修复 2026-10-28）

- ~~版本号未随功能演进~~：已升 1.3.0（tauri.conf.json / Cargo.toml 同步），顶栏徽标改读
  `app_version` IPC（单一来源 conf，前端 Mock 分支返回 `0.0.0-mock`）。
- ~~状态栏数据目录路径不可见~~：`.dir` 的 `flex-basis: 0` 改 `auto`——原值使基准宽度恒为 0，
  路径永远渲染不出来；改后按内容取宽、空间不足时收缩截断（浏览器实测可见）。

---

## 附录 A：评审与实现决策记录

对 v0.1 原稿的评审结论：**整体可行，以下几点需要修正或拍板，均已处理**：

1. **中文检索**（原稿 §3.4）：FTS5 默认分词不支持中文 → 改为 LIKE 候选 + 权重排序，FTS5 表保留同步（见 §3.4 修正说明）。
2. **技术栈**：应用户要求不做 Python，直接 **Rust 桌面应用**；选 Tauri 2 而非 egui，因为原稿 v1.0 目标就是 Tauri，且 webview 对中文渲染/CSS 排版零成本，UI 设计稿可直接复用。
3. **前端无构建**：Vite 构建链砍掉，Vue3 用官方全局构建本地 vendor。用户构建只需 Rust 工具链。
4. **patterns 表**补 `UNIQUE(base_cmd, suffix)`，组合使用量可累加。
5. **复制埋点闭环**：原稿权重公式依赖「用过 N 次 / 7 天内」，但没写数据从哪来 → 明确由复制动作回写（§4.5）。
6. **种子数据**：首发附带 docker logs 等参数树、6 条示例个人记录（标注可删）、5 页内置中文 tldr 页，开箱即有内容；正式 tldr 库由导入脚本按目录批量导入。
7. **安全**：无网络监听（纯本地 webview IPC），数据目录可迁移（`SNIPNAV_HOME`）。
8. **项目维度**（用户澄清核心痛点后新增）：`snippets.project` 记录命令归属项目，CLI 存入时自动取当前目录名，GUI 可按项目过滤；搜索时同项目记录 +30 分。理由：同一项目的命令高度复用，"我在这个项目里跑过什么"比全局搜索更接近真实提问。
9. **参数提示不依赖全量 man 数据**：分三层——① 手写参数树覆盖自己最常用的 ~20 个命令；② tldr 精选页覆盖广度（全库 1.2 万条约 5-10MB，正是"man 的精选版"）；③ v0.2 按需解析 `cmd --help` 生成参数树（只对自己机器上实际安装的版本、按需增量，零预置体积）。**不收全量 man**：100-200MB 体积、信噪比低（man 是参考手册不是发现工具）、Windows 本地也没有 man。
10. **SQLite 体积与速度**（用户关注点）：个人 500 条 ≈ 200KB；tldr 全库导入后整库 ≈ 5-10MB；参数树 <100KB。查询：主键/索引 O(log n)，LIKE 候选在万级行上毫秒级，Rust 侧打分微秒级。控体积手段：external-content FTS（不复制正文）、WAL、删除密集后 `VACUUM` 回收、不存 BLOB。结论：按需生长的库天然小，唯一会撑爆它的就是"全量文档入库"，而那正是我们不做的事。
11. **v0.2 实现要点**：① helpgen（`--help` 抓取→参数树）只在用户显式触发时运行目标命令，工具名/子命令字符白名单（拒路径分隔符）、8 秒超时强杀、输出 256KB 上限、Windows `CREATE_NO_WINDOW` 防闪黑框、git 特判用 `-h`（`git --help` 会拉起 man 分页器）；解析兼容 GNU/docker/kubectl（2+ 空格对齐）与 Windows curl（1 空格缩进）；与手写参数树按长参数名去重，未归类参数进「其他」组。② 性能三项：搜索候选标签改为单条 `IN` 批量查询（消灭 N+1）；tldr 目录导入包显式事务（万级示例从每语句一落盘变为一次提交）；`meta` 计数表 + 触发器维护行数（O(1) 判断 FTS 切换，阈值 2 万行，关键词全 ASCII 且 ≥3 字符才走 FTS5 MATCH，中文/短词永远 LIKE 保底）。
12. **v1.1 性能优化包（四条，针对"若导入 100-200MB man 全量"的预案）**：① **结构化文档导入** `snip import-docs`（docs.rs）——手册页解析出「名称+一句简介」作可搜索小行（source='doc'），**整页原文进 `doc_bodies` 旁表**，点卡片「📖 全文」按 id 取（O(1)），可搜索数据量与语料体积解耦（~20×）；② **中文/短词路由拆分**——大库双路合并：FTS（ASCII≥3 字符词）查全库含 doc 层，LIKE（全部词）**只扫 self/tldr/ai 自有数据**，中文词不再白扫英文语料（~20×，单测验证 doc 层不被中文命中）；③ **PRAGMA 调优** `cache_size=-65536`（64MB 页缓存）+ `mmap_size=256MB`（零拷贝读），每连接生效（2-5×）；④ **旁表永不进搜索/FTS 索引**——FTS 触发器只同步 snippets 基础列，旁表天然隔离，索引体积不随语料增长。结论：四条落地后，即便导入全量 man（100-200MB），所有查询路径依然毫秒级。

## 附录 B：项目结构与模块职责

```
snipnav/
├── DESIGN.md                  # 本文档
├── README.md                  # 构建 / 使用说明
├── frontend/                  # UI（静态文件，无构建）
│   ├── index.html
│   ├── style.css
│   ├── js/api.js              # invoke 封装 + 浏览器 Mock（开发用）
│   ├── js/app.js              # Vue3 界面
│   └── vendor/vue.global.prod.js
└── src-tauri/
    ├── Cargo.toml / build.rs / tauri.conf.json
    ├── capabilities/default.json
    ├── icons/icon.ico
    ├── seeds/tldr_zh/*.md     # 内置中文速查页（编译期嵌入）
    └── src/
        ├── main.rs            # GUI 入口（windows_subsystem）
        ├── lib.rs             # 模块组织 + run_gui()（托盘/关窗隐藏/自启插件注册）
        ├── bin/snip.rs        # CLI 入口（snip add / search / …）
        ├── db.rs              # 连接 / 建表（含 meta/doc_bodies/settings）/ 通用读写
        ├── models.rs          # serde 数据结构（camelCase 出参）
        ├── tagging.rs         # 自动标签 / tool-command 推导
        ├── search.rs          # 候选检索（LIKE/FTS 双路）+ 权重排序
        ├── explore.rs         # 参数发散面板数据组装
        ├── importer.rs        # tldr 页解析与目录导入
        ├── helpgen.rs         # v0.2：--help 抓取与解析（白名单/超时/参数树入库）
        ├── docs.rs            # v1.1：批量手册导入（man），doc_bodies 旁表读写
        ├── history.rs         # v0.3：shell hook 安装 / history.log 读取与裁剪
        ├── share.rs           # v0.4：export / import-share 团队共享合并
        ├── llm.rs             # v0.5：AI 兜底（OpenAI 兼容接口调用与入库）
        ├── seeds.rs           # 参数树 / 示例记录 / 内置页种子
        └── commands.rs        # Tauri command 层（IPC 边界）
```
