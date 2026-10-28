# SnipNav 🧭 本地命令速查与智能发散

离线桌面应用：**你跑通过的命令，存一次，下次一搜即得**。同一个项目的命令优先展示；点开任意命令，参数发散面板告诉你「这条命令还能怎么加参数」。不连云、不执行命令、数据全在本机。

设计文档见 [DESIGN.md](DESIGN.md)。

## 功能

- 🔍 **人话搜索**：按「docker 日志 过滤」这样的自然语言搜，权重排序（自己的记录 / 同项目 / 用过次数 / 7 天内用过 / 标签命中）
- 📁 **项目维度**：CLI 存入自动归属当前项目，GUI 一键按项目过滤
- 📋 **点开即复制**：复制自动累计使用次数，反哺排序
- 🔀 **参数发散面板**：参数树分组展示（时间/输出/过滤/管道）+ 你的常用组合 + 相关命令；点选参数拼「当前组合」，可复制或存为新记录
- ⚡ **--help 参数树自动生成**（v0.2）：面板一键「用 --help 补充参数」，自动运行工具帮助并解析入库（增量去重、8 秒超时、工具名白名单）；CLI 同款 `snip gen-params docker logs`
- 🏷 **自动打标签**：从命令文本规则提取（docker/k8s/git/windows/log/filter…）
- 🌗 **明亮/暗黑主题**：顶栏一键切换，localStorage 记忆偏好，暗黑为默认
- 🕘 **Shell 历史采集**（v0.3）：`snip hook --install` 后自动记录执行过的命令，GUI 历史弹窗挑有用的入库；`snip stats` 看 Top5 常用
- 🤝 **团队共享**（v0.4）：`snip export share.json` 导出个人记录，队友 `snip import-share share.json` 智能合并（同命令保留较大使用量、合并标签）
- 🤖 **AI 兜底**（v0.5，**默认关闭**）：设置里显式开启并配好 Key 后，搜索无结果时一键让大模型生成（OpenAI 兼容接口，默认 GLM），自动入库打 🤖 徽标、可删；不开启则完全不调用
- 📚 **批量手册导入**（v1.1）：`snip import-docs <目录>` 导入 man 等纯文本手册页——页名+简介作可搜索记录，整页原文存旁表（点「📖 全文」查看，不占搜索索引），选项段落自动并入参数树
- 📌 **托盘 + 开机自启 + 安装包**（v1.x）：关窗隐藏到托盘；设置里可开自启；`tauri build` 出 NSIS 安装包
- 📖 **tldr 中文速查**：内置 10 页种子（grep/curl/tar/jq/docker logs/mvn/gradle/sqlite3/mysql/psql）；支持导入任意 tldr 格式仓库（官方英文库或社区中文翻译版）
- 🖥 **CLI 伴随工具**：终端里一行存命令、搜命令

## 构建与运行

依赖：Rust（msvc 工具链）+ VS C++ Build Tools。数据存于 `~/.snipnav/`（环境变量 `SNIPNAV_HOME` 可改）。

```bash
cd src-tauri

# 开发运行（会弹出应用窗口）
cargo run

# 发布构建（单 exe，位于 target/release/）
cargo build --release

# 单元测试
cargo test
```

发布版 exe：`src-tauri/target/release/SnipNav.exe`（无控制台窗口）；CLI 为同目录下 `snip.exe`。想打安装包：`cargo install tauri-cli` 后 `cargo tauri build`（NSIS）。

## CLI 用法（snip.exe）

```bash
# 存一条（自动归属当前目录名到项目）
snip add "docker 日志按时间过滤错误" "docker logs -f web --since 1h | grep ERROR" --tag docker,log
snip add "打包前端" "pnpm build" --project tools-help --note "记得先装依赖"

# 搜（可限项目）
snip search "docker 日志"
snip search "端口" --project tools-help

# 新工具参数树：跑 --help 自动生成（增量，可重复）
snip gen-params git log
snip gen-params curl

# Shell 历史采集（Git Bash）
snip hook --install      # 幂等，装到 ~/.bashrc；重开终端生效
snip history             # 看最近执行的命令
snip adopt 3 "查容器日志"  # 把第 3 条入库（自动归属当前目录为项目）

# 团队共享
snip export share.json
snip import-share share.json

# 批量手册导入（man 等，可选）：整页原文存旁表不占搜索索引，选项并入参数树
# Linux/WSL 导出示例：man -P cat docker-logs > docker-logs.txt
snip import-docs C:\repo\man-pages

# 其他
snip list              # 最近/常用
snip stats             # 库统计 + Top5 常用
snip delete <id>
snip vacuum            # 压缩数据库（大量删除后用）
snip import-tldr C:\repo\tldr\pages    # 导入 tldr 仓库
```

## 导入 tldr 速查库（可选）

```bash
git clone --depth 1 https://github.com/tldr-pages/tldr.git
snip import-tldr tldr/pages        # 或在 GUI 设置里填路径
```

导入是增量的，重复执行自动跳过已有序目；只导 `pages/common` 可减半体积。

## 前端开发（浏览器 Mock 模式）

前端是纯静态文件（Vue3 全局构建，无 Node 构建步骤）。用浏览器直接打开 `frontend/index.html` 即可开发调试——检测不到 Tauri 环境时会自动切换内置 Mock 数据。

## 目录结构

```
snipnav/
├── DESIGN.md              # 设计文档（含实现决策记录、性能/存储优化预案）
├── README.md
├── frontend/              # UI：index.html / style.css / js/{api,app}.js / vendor/vue
├── scripts/make_icon.py   # 重新生成图标
└── src-tauri/             # Rust：核心逻辑 + Tauri 壳
    ├── tauri.conf.json
    ├── seeds/tldr_zh/     # 内置中文速查页（编译期嵌入）
    └── src/
        ├── main.rs        # GUI 入口
        ├── lib.rs         # 模块组织 + 托盘/关窗隐藏
        ├── bin/snip.rs    # CLI 入口
        ├── db.rs          # SQLite（rusqlite bundled，meta/doc_bodies/settings 等表）
        ├── models.rs      # IPC 数据结构（serde camelCase）
        ├── search.rs      # 检索 + 权重排序（LIKE/FTS 双路）
        ├── explore.rs     # 参数发散面板
        ├── helpgen.rs     # --help 抓取解析（v0.2 参数树自动生成）
        ├── docs.rs        # 批量手册导入 + 全文旁表（v1.1）
        ├── history.rs     # shell 历史采集钩子（v0.3）
        ├── share.rs       # export / import-share 团队共享（v0.4）
        ├── llm.rs         # AI 兜底生成（v0.5）
        ├── importer.rs    # tldr 解析/导入
        ├── tagging.rs     # 自动标签 / tool-command 推导
        ├── seeds.rs       # 参数树 / 示例 / 内置页种子
        └── commands.rs    # Tauri IPC 命令层
```
