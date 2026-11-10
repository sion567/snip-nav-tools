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
- 🤖 **AI 兜底**（v0.5，**默认关闭**）：设置里显式开启并配好 Key 后，搜索无结果时一键让大模型生成（OpenAI 兼容接口，默认 GLM），自动入库打 🤖 徽标、可删；不开启则完全不调用；**本地 Ollama 免 Key**（v1.10）：设置里点「检测本地 Ollama」自动填地址拉模型列表，彻底离线
- 🔄 **Git 仓库同步**（v1.10）：`snip sync <仓库地址>` 拉取合并→导出→提交推送，团队共用一个私有 git 仓库即可协作；每条记录一个 md 文件（frontmatter），diff 友好；仓库内置 .gitattributes 强制 LF
- 📚 **批量手册导入**（v1.1）：`snip import-docs <目录>` 导入 man 等纯文本手册页——页名+简介作可搜索记录，整页原文存旁表（点「📖 全文」查看，不占搜索索引），选项段落自动并入参数树
- 📚 **文档库浏览**（v1.4）：顶栏「📚 文档库」或搜索结果上的「📖 全部文档」——按来源分组（你的记录 / AI / tldr / 手册）列出关键词命中的**全部**条目，不按热度截断；空查询展示工具索引（库里有哪些工具、各多少条）。CLI 同款 `snip docs docker`
- 📌 **托盘 + 开机自启 + 安装包**（v1.x）：关窗隐藏到托盘；设置里可开自启；`tauri build` 出 NSIS 安装包
- ⚡ **全局快捷键速查浮窗**（v1.5）：`Alt+Space`（可在设置改/禁用）呼出置顶小窗——打字即搜、↑↓ 选择、Enter 复制后自动关闭，全程手不离键盘；托盘菜单 ⚡ 同样可呼出
- ✏️ **参数占位符填空**（v1.5）：识别命令里的 `{{容器}}` / `<容器>` 占位符，发散面板里逐个填值实时替换预览；没填完复制会被拦下
- 📌 **收藏置顶 Pin**（v1.5）：卡片一键 📌，搜索排序恒置顶；CLI `snip pin <id>` 切换，团队共享文件同步置顶状态
- 🖥 **PowerShell 历史采集**（v1.5）：自动读取 PSReadLine 历史文件（无需装钩子），与 Git Bash 钩子记录合并进历史弹窗
- ⚠️ **危险命令标注**（v1.6）：`rm -rf` / `git reset --hard` / `drop table` / `docker system prune` 等自动标 ⚠️ 并注明风险（只提醒不拦截）
- 📋 **剪贴板监听入库**（v1.6，默认关闭）：设置开启后，复制到像命令的内容时窗口顶部提醒一键入库；识别表动态含库里出现过的工具；完全本地
- 🔍 **命令逐段解释**（v1.7）：发散面板里命令按管道分段、flag 悬浮显示参数树描述（explainshell 式），没收录的参数黄虚线提示可 ⚡ 补充
- 📓 **运行手册 Playbook**（v1.7）：把部署/排障流程的多条命令按顺序打包，一键复制全部步骤（每步计一次使用）；步骤搜索挑选、可排序
- 🕐 **动态变量**（v1.7）：命令里的 `{{date}}` / `{{time}}` / `{{year}}` 复制时自动展开为当前值
- 💾 **数据库自动备份**（v1.7）：启动时滚动备份到 `~/.snipnav/backup/`（每天一份，保留 7 份，SQLite 在线备份 API 对 WAL 安全）
- 🈶 **拼音搜索**（v1.8）：意图是汉字也能用拼音搜——"rizhi" / "rz" 都能命中「日志」；全拼与首字母两列，老库自动回填
- 📥 **复制并粘贴回原窗口**（v1.8，默认关）：设置开启后，浮窗里 Enter = 复制并粘贴回呼出前的窗口（模拟一次 Ctrl+V，命令仍由你在终端自己按回车）
- 📊 **使用回顾热力图**（v1.9）：顶栏「📊 回顾」看近一年 GitHub 式格子图（复制/组合/存变体/收藏都算），格子越绿用得越勤
- 🌐 **工作流导入**（v1.9）：`snip import-workflows <路径>` 或设置里导入 Warp Workflows / Espanso 包 / VS Code snippets，格式自动识别；Warp/Espanso 的 `{{var}}` 直接接上占位符填空，VS Code 的 `$1`/`${2:默认}` 转成 `<1>`/`<默认>`
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

# 浏览某工具的全部文档（按来源分组，不截断）；无参数 = 工具索引
snip docs docker
snip docs

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

# 收藏置顶（再执行一次取消）
snip pin 3

# 团队 Git 同步（每条记录一个 md，diff 友好；删除不传播）
snip sync git@github.com:team/snipnav-sync.git

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
