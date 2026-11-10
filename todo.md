# SnipNav 待办清单

> 慢慢补，做完勾掉（`[ ]` → `[x]`）。有新想法直接往对应分区追加。
> 性能类条目与 DESIGN.md §11 对应，那边有完整分析。

## 🔧 小修（半小时内，优先清掉）

- [x] **版本号跟上功能**：`tauri.conf.json` / `Cargo.toml` 升 1.3.0；顶栏徽标改读
      `app_version` IPC（commands.rs，单一来源 conf）。✅ 2026-10-28
- [x] **状态栏数据目录不可见**：`style.css` `.dir` 的 `flex-basis: 0` → `auto`
      （按内容取宽、空间不足收缩截断）。浏览器实测可见宽 116px，长路径正常截断。✅ 2026-10-28
- [x] **QUICK_TAGS 挤占动态标签**：合并上限 10 → 12，动态高频在前、静态只补位。✅ 2026-10-28

## 🧹 仓库卫生

- [x] 删手工测试残留 `.road-test/`（已 `git rm`）、空目录 `.docs-test/`。✅ 2026-10-28
- [ ] 三个历史提交可考虑 squash 成一个干净的首版（如果还没推远端）。

## 📚 手册 / 种子内容（本轮主题的延续）

- [ ] 内置页继续扩：kubectl / ffmpeg / ssh / sed / awk / powershell / npm / pnpm / cargo…
      每页 5-8 条高频示例，格式照 `src-tauri/seeds/tldr_zh/*.md`，
      注册进 `BUILTIN_TLDR` 后 `TLDR_SEED_VERSION` +1。
- [ ] PARAM_SEEDS 手写参数树补高频工具：git stash / rebase、docker compose、
      kubectl get/describe、curl 进阶组合。
- [ ] 本机跑 `snip gen-params mvn`、`snip gen-params sqlite3` 等，用真实 `--help`
      补全新工具的参数树（比手写全，且贴合本机版本）。
- [ ] 导入官方 tldr 全库拿最大覆盖面（~1.2 万条，5-10MB）：
      `git clone --depth 1 https://github.com/tldr-pages/tldr.git && snip import-tldr tldr/pages`。

## 🌟 功能 Roadmap（2026-11-08 同类工具评审）

> 对标项目：**navi**（参数占位符填空）、**Warp Workflows**（命令模板/危险命令标注）、
> **Raycast / uTools / Flow Launcher**（全局快捷键呼出、剪贴板监听）、
> **Espanso**（文本扩展、动态变量）、**Atuin**（历史统计）、**pet / Cacher**（同步/团队）、
> **massCode / SnippetsLab**（收藏/组织）。
> 四条设计原则不变：记录成本趋近零 / 搜索靠人话 / 发散不猜 / 本地优先。

### 第一梯队（本轮实现，小改动高价值）

- [x] **全局快捷键速查浮窗**（Raycast/uTools 的灵魂交互）：`Alt+Space` 呼出置顶小窗
      → 打字搜 → Enter 复制 → 窗口消失，全程不碰鼠标。独立 quick 页面 +
      tauri-plugin-global-shortcut；快捷键可在设置里改。✅ 2026-11-08
- [x] **参数占位符填空**（navi/Warp 式）：识别命令里的 `{{容器}}` / `<容器>` 占位符，
      面板里逐个填值实时替换预览，复制的是填好的最终命令；未填完给出提醒。✅ 2026-11-08
- [x] **收藏/置顶 Pin**（massCode/SnippetsLab）：snippets 加 `pinned` 列，
      卡片一键 📌，搜索排序 +500 恒置顶；CLI `snip pin <id>` 切换。✅ 2026-11-08
- [x] **PowerShell 历史采集**（补 Windows 半边天）：PSReadLine 自带历史文件
      `%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`，
      读文件即可，无需装钩子；与 Git Bash 历史合并进历史弹窗（带 ⚡ 来源标记）。✅ 2026-11-08

### 第二梯队（✅ 2026-11-09 全部完成）

- [x] **剪贴板监听入库**（Raycast/Ditto）：检测到复制内容像命令（首 token 命中已知
      工具名）时提醒"存入 SnipNav？"——覆盖 GUI 里复制的命令，与 shell hook 互补。
      实现：后台线程 2s 轮询（默认关闭，设置开关即时生效），主窗口横幅一键存/忽略。✅ 2026-11-08
- [x] **危险命令 ⚠️ 标注**（Warp）：`rm -rf` / `git reset --hard` / `drop table`
      规则表标红，tagging.rs 同款机制。实现：读行时即时计算不落库，规则改进老记录自动跟上。✅ 2026-11-08
- [x] **命令解释模式**（explainshell 式）：已存命令逐 flag 悬浮显示 param_tree 描述。
      实现：explore.rs build_explain 按管道分段，子命令级优先不跨子命令借用，
      支持 --flag=value 与合并短旗标；绿虚线=有说明、黄虚线=未收录（⚡ 可补）。✅ 2026-11-09
- [x] **Playbook 多命令组合**：部署/排障的 5-6 条命令打包成有序步骤，一键逐步复制
      （直击"团队交接靠口头"痛点）。
      实现：playbooks + playbook_items 两表，📓 手册弹窗（编辑器带步骤搜索/排序），
      复制全部逐行拼接且每步计一次使用。✅ 2026-11-09
- [x] **动态变量**（Espanso 式）：复制时展开 `{{date}}`/`{{time}}`/`{{year}}`
      （{{clipboard}} 涉及剪贴板读权限，暂缓）。与占位符填空共存，auto 占位不算"未填"。✅ 2026-11-09
- [x] **复制并粘贴到原窗口**（默认关闭的开关）。
      实现：只在浮窗内生效——呼出瞬间 GetForegroundWindow 记目标，Enter 写剪贴板→
      藏浮窗→320ms 后模拟 Ctrl+V（AttachThreadInput 兜底）；不代跑命令，用户终端自己按回车。✅ 2026-11-09
- [x] **拼音搜索**：意图存的是汉字，手打成拼音（"docker rizhi"）也能搜到。
      实现：intent_py/intent_pyi 两列（pinyin crate），CAND_COND 9 路 LIKE，
      打分 +4；插入/更新写入 + 老库一次性回填（meta 标记，中断可续跑）。✅ 2026-11-09
- [x] **数据库自动备份**：启动时滚动备份 snippets.db 到 `~/.snipnav/backup/`（留 N 份）。
      实现：open() 时每天一份 snippets-YYYYMMDD.db 保留 7 份，SQLite 在线备份 API 对 WAL 安全，
      失败静默不阻塞启动/CLI。✅ 2026-11-09

### 第三梯队（大件，需求出现再动）

- [x] **Ollama 本地模型接入**：AI 兜底加本地选项，彻底贯彻"不连云"。
      实现：本地端点（11434/localhost）免 Key 不带鉴权头；设置页「检测本地 Ollama」
      一键填地址 + 拉 OpenAI 兼容 /models 模型列表（datalist 选择）。✅ 2026-11-10
- [x] **Git 仓库同步**（pet Gist 式）：`snip sync` 推私有 git 仓库，diff 友好，
      比 JSON 导入导出更适合团队长期协作。
      实现：每条记录一个 fnv(cmd).md（frontmatter+正文命令），clone/pull→合并
      →导出→commit→push；踩平 Windows git 三坑（autocrlf CRLF 解析、
      .gitattributes 随首提交、幽灵修改用 diff --cached 判断）。✅ 2026-11-10
      已知取舍：删除不传播。
- [x] **使用热力图**（Atuin/GitHub 式）：interactions 埋点已积累，做年度热力图。
      实现：heatmap IPC 返回近一年稀疏日计数，前端周列网格（周日对齐、1 号挂
      月份标签、max/4 分档），顶栏「📊 回顾」弹窗。✅ 2026-11-10
- [x] **导入生态**：Warp Workflows YAML / Espanso 包 / VS Code snippets 导入。
      实现：workflows.rs 单入口按扩展名+内容特征自动识别，目录递归、按 cmd 去重；
      Warp/Espanso 的 {{var}} 直接接占位符填空，VS Code 的 $1/${2:默认}→\<1\>/\<默认\>；
      GUI 设置页 + `snip import-workflows` 双入口。✅ 2026-11-10
- [ ] 内网多用户实时共用库（v0.4 明确延后的服务化）。

## 🚀 性能预案（DESIGN.md §11.3/§11.4，导入 100MB 级语料前再做）

- [ ] P1#5：doc_bodies 压缩存储（zstd/deflate，1 字节格式头，懒迁移）。
- [ ] P1#6：FTS 延迟创建（行数过阈值才建表回填，小库省索引体积）。
- [ ] P2 记录在案暂不做：interactions 增长治理、增量 VACUUM、LIKE 候选封顶。

## 🧪 质量保障（可选）

- [ ] 前端最小冒烟：Mock 模式下 搜索→出卡片→点参数组合→复制 一条链路。
- [ ] 远端仓库 + CI（GitHub Actions windows-latest 跑 `cargo test`），推远端后再说。
