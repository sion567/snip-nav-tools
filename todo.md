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

## ✨ 功能增强（按价值排序，做前先聊）

- [ ] **全局快捷键呼出**：现在必须点托盘/任务栏。Tauri global-shortcut 插件，
      如 `Alt+Space` 直接弹出主窗口并聚焦搜索框。
- [ ] **PowerShell 历史采集**：v0.3 hook 只支持 Git Bash（PROMPT_COMMAND）。
      PSReadLine 自带历史文件
      `%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`，
      历史弹窗里加一个"导入 PowerShell 历史"入口即可，不用装钩子。
- [ ] **拼音搜索**：意图存的是汉字，但手可能打成拼音（"docker rizhi"）。
      搜索层加简易拼音首字母/全拼映射做二级匹配。
- [ ] **数据库自动备份**：snippets.db 是积累出来的个人资产，现在只有手工 export。
      启动时滚动备份到 `~/.snipnav/backup/`（保留最近 N 份，几 KB 成本）。
- [ ] 内网多用户实时共用库（v0.4 明确延后的服务化）——需求出现再动。

## 🚀 性能预案（DESIGN.md §11.3/§11.4，导入 100MB 级语料前再做）

- [ ] P1#5：doc_bodies 压缩存储（zstd/deflate，1 字节格式头，懒迁移）。
- [ ] P1#6：FTS 延迟创建（行数过阈值才建表回填，小库省索引体积）。
- [ ] P2 记录在案暂不做：interactions 增长治理、增量 VACUUM、LIKE 候选封顶。

## 🧪 质量保障（可选）

- [ ] 前端最小冒烟：Mock 模式下 搜索→出卡片→点参数组合→复制 一条链路。
- [ ] 远端仓库 + CI（GitHub Actions windows-latest 跑 `cargo test`），推远端后再说。
