//! snip —— 命令行伴随工具：终端里一行存命令、搜命令。
//!
//! 用法：
//!   snip add "描述" "命令" [--tag a,b] [--project 名称] [--note 备注]
//!   snip search "关键词" [--project 名称]
//!   snip list [--limit N]
//!   snip stats
//!   snip import-tldr <目录>
//!   snip delete <id>
//!
//! add 不带 --project 时自动取当前目录名作为项目归属。

use snipnav::db::{self, NewSnippet};
use snipnav::importer;
use snipnav::models::Hit;
use snipnav::{search, tagging};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let rest: &[String] = if args.is_empty() { &[] } else { &args[1..] };
    let code = match cmd.as_str() {
        "add" => cmd_add(rest),
        "search" | "s" => cmd_search(rest),
        "list" | "ls" => cmd_list(rest),
        "stats" => cmd_stats(),
        "import-tldr" => cmd_import(rest),
        "gen-params" => cmd_gen_params(rest),
        "import-docs" => cmd_import_docs(rest),
        "vacuum" => cmd_vacuum(),
        "hook" => cmd_hook(rest),
        "history" => cmd_history(rest),
        "adopt" => cmd_adopt(rest),
        "export" => cmd_export(rest),
        "import-share" => cmd_import_share(rest),
        "delete" | "rm" => cmd_delete(rest),
        "help" | "--help" | "-h" | "" => {
            print_help();
            0
        }
        _ => {
            eprintln!("未知命令: {cmd}（snip help 查看用法）");
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    println!(
        "snip - SnipNav 命令行\n\
         \x20 snip add \"描述\" \"命令\" [--tag a,b] [--project 名称] [--note 备注]\n\
         \x20 snip search \"关键词\" [--project 名称]\n\
         \x20 snip list [--limit N]\n\
         \x20 snip gen-params <工具> [子命令]   # 跑 --help 自动生成参数树\n\
         \x20 snip import-tldr <tldr仓库pages目录>\n\
         \x20 snip import-docs <手册页目录>       # man 等纯文本手册（man -P cat 导出）\n\
         \x20 snip hook [--install]             # 安装 bash 命令记录钩子\n\
         \x20 snip history [--limit N]          # 查看最近执行的命令\n\
         \x20 snip adopt <序号> \"描述\" [--tag]  # 把历史命令入库\n\
         \x20 snip export <文件.json>           # 导出个人记录（团队共享）\n\
         \x20 snip import-share <文件.json>     # 导入他人共享（智能合并）\n\
         \x20 snip vacuum                       # 压缩数据库文件\n\
         \x20 snip stats / snip delete <id>"
    );
}

fn cmd_add(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("用法: snip add \"描述\" \"命令\" [--tag a,b] [--project 名称] [--note 备注]");
        return 2;
    }
    let intent = args[0].trim();
    let cmd = args[1].trim();
    let mut tags: Vec<String> = Vec::new();
    let mut project: Option<String> = None;
    let mut note: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--tag" | "-t" if i + 1 < args.len() => {
                tags.extend(tagging::split_tags(&args[i + 1]));
                i += 2;
            }
            "--project" | "-p" if i + 1 < args.len() => {
                project = Some(args[i + 1].clone());
                i += 2;
            }
            "--note" | "-n" if i + 1 < args.len() => {
                note = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                eprintln!("无法识别的参数: {}", args[i]);
                return 2;
            }
        }
    }
    let project = project.or_else(current_project);
    match snip_add(intent, cmd, tags, project.as_deref(), note.as_deref()) {
        Ok(id) => {
            println!("已保存 #{id}: {intent}");
            println!("  {cmd}");
            if let Some(p) = &project {
                println!("  📁 {p}");
            }
            0
        }
        Err(e) => {
            eprintln!("保存失败: {e}");
            1
        }
    }
}

fn current_project() -> Option<String> {
    std::env::current_dir()
        .ok()?
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
}

fn snip_add(
    intent: &str,
    cmd: &str,
    tags: Vec<String>,
    project: Option<&str>,
    note: Option<&str>,
) -> Result<i64, String> {
    if intent.is_empty() || cmd.is_empty() {
        return Err("描述和命令都不能为空".into());
    }
    let conn = db::open()?;
    let (tool, command) = tagging::derive_tool_command(cmd);
    let merged = tagging::merge_tags(Some(&tags), cmd, intent);
    let id = db::insert_snippet(
        &conn,
        NewSnippet {
            intent,
            cmd,
            project,
            tool: tool.as_deref(),
            command: command.as_deref(),
            variant_of: None,
            source: "self",
            verified: true,
            used_count: 0,
            last_used: None,
            notes: note,
        },
    )?;
    db::set_tags(&conn, id, &merged)?;
    Ok(id)
}

fn parse_limit(args: &[String], default: usize) -> usize {
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "--limit" || args[i] == "-l") && i + 1 < args.len() {
            if let Ok(n) = args[i + 1].parse::<usize>() {
                return n.clamp(1, 200);
            }
        }
        i += 1;
    }
    default
}

fn cmd_search(args: &[String]) -> i32 {
    let mut q = String::new();
    let mut project: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "--project" || args[i] == "-p") && i + 1 < args.len() {
            project = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if args[i].starts_with("--") {
            i += 1;
            continue;
        }
        if !q.is_empty() {
            q.push(' ');
        }
        q.push_str(&args[i]);
        i += 1;
    }
    match run_search(&q, project.as_deref(), 20) {
        Ok(hits) => {
            print_hits(&hits);
            0
        }
        Err(e) => {
            eprintln!("搜索失败: {e}");
            1
        }
    }
}

fn cmd_list(args: &[String]) -> i32 {
    let limit = parse_limit(args, 30);
    match run_search("", None, limit) {
        Ok(hits) => {
            print_hits(&hits);
            0
        }
        Err(e) => {
            eprintln!("读取失败: {e}");
            1
        }
    }
}

fn run_search(q: &str, project: Option<&str>, limit: usize) -> Result<Vec<Hit>, String> {
    let conn = db::open()?;
    search::search(&conn, q, project, limit)
}

fn print_hits(hits: &[Hit]) {
    if hits.is_empty() {
        println!("（无匹配记录）");
        return;
    }
    for h in hits {
        let mark = match h.sn.source.as_str() {
            "self" => "🟢",
            "tldr" => "📖",
            _ => "📄",
        };
        println!("[{:>4}] {} ({}分, 用过{}次) {}", h.sn.id, mark, h.score, h.sn.used_count, h.sn.intent);
        println!("        {}", h.sn.cmd);
        let mut extra: Vec<String> = Vec::new();
        if let Some(p) = &h.sn.project {
            extra.push(format!("📁{p}"));
        }
        if !h.sn.tags.is_empty() {
            extra.push(format!("🏷 {}", h.sn.tags.join(",")));
        }
        if !extra.is_empty() {
            println!("        {}", extra.join("  "));
        }
    }
}

fn cmd_stats() -> i32 {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    println!(
        "共 {} 条 | 个人 {} | tldr {} | 文档 {}",
        db::count(&conn, "snippets"),
        conn.query_row("SELECT COUNT(*) FROM snippets WHERE source='self'", [], |r| r.get::<_, i64>(0)).unwrap_or(0),
        conn.query_row("SELECT COUNT(*) FROM snippets WHERE source='tldr'", [], |r| r.get::<_, i64>(0)).unwrap_or(0),
        conn.query_row("SELECT COUNT(*) FROM snippets WHERE source='doc'", [], |r| r.get::<_, i64>(0)).unwrap_or(0),
    );
    // v0.3：行为统计——最常用 Top5
    let mut stmt = match conn.prepare(
        "SELECT intent, used_count FROM snippets WHERE source IN ('self','ai') AND used_count > 0 \
         ORDER BY used_count DESC LIMIT 5",
    ) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let top: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    if !top.is_empty() {
        println!("最常用：");
        for (intent, used) in top {
            println!("  {:>3} 次  {}", used, intent);
        }
    }
    match db::db_path() {
        Ok(p) => println!("数据目录: {}", p.display()),
        Err(e) => eprintln!("{e}"),
    }
    0
}

fn cmd_import(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("用法: snip import-tldr <tldr仓库pages目录>");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let p = importer::expand_path(path);
    match importer::import_dir(&conn, &p) {
        Ok(r) => {
            println!("导入完成：{} 个页面文件，新增 {} 条速查，跳过重复 {} 条", r.pages, r.snippets, r.skipped);
            0
        }
        Err(e) => {
            eprintln!("导入失败: {e}");
            1
        }
    }
}

// ---------- v1.1：批量手册导入（man 等） ----------

fn cmd_import_docs(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("用法: snip import-docs <手册页目录>");
        eprintln!("语料：纯文本手册页（.txt/.md/.1~.8），如 Linux/WSL 上 man -P cat docker-logs > docker-logs.txt");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let p = snipnav::importer::expand_path(path);
    match snipnav::docs::import_docs_dir(&conn, &p) {
        Ok(r) => {
            println!("导入完成：{} 个手册页，跳过已有 {} 个", r.pages, r.skipped);
            println!("整页原文存旁表（不参与搜索索引），选项已并入参数树；搜索页名即可找到。");
            0
        }
        Err(e) => {
            eprintln!("导入失败: {e}");
            1
        }
    }
}

fn cmd_gen_params(args: &[String]) -> i32 {
    let Some(tool) = args.first() else {
        eprintln!("用法: snip gen-params <工具> [子命令]   例: snip gen-params docker logs");
        return 2;
    };
    let sub = args.get(1).map(|s| s.as_str());
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    match snipnav::helpgen::generate_into(&conn, tool, sub) {
        Ok((parsed, added)) => {
            let scope = sub.map(|s| format!("{tool} {s}")).unwrap_or_else(|| tool.clone());
            println!("{scope}：解析 {parsed} 个参数，新增 {added} 个（其余与已有参数树重复跳过）");
            println!("在应用里展开该命令即可看到新参数。");
            0
        }
        Err(e) => {
            eprintln!("生成失败: {e}");
            1
        }
    }
}

// ---------- v0.3：hook / history / adopt ----------

fn cmd_hook(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--install") {
        return match snipnav::history::install_hook() {
            Ok(true) => {
                println!("✅ 已安装到 ~/.bashrc（新开终端生效；重装 Git Bash 不会影响）");
                println!("   钩子会把执行过的命令记到 history.log，用 snip history 查看。");
                0
            }
            Ok(false) => {
                println!("已安装过，跳过。");
                0
            }
            Err(e) => {
                eprintln!("安装失败: {e}");
                1
            }
        };
    }
    print!("{}", snipnav::history::hook_script());
    println!("# 手动安装：把上面内容追加到 ~/.bashrc，或直接执行 snip hook --install");
    0
}

fn cmd_history(args: &[String]) -> i32 {
    let limit = parse_limit(args, 30);
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let entries = snipnav::history::read_recent(&conn, limit);
    if entries.is_empty() {
        println!("（暂无历史记录；先执行 snip hook --install 安装采集钩子）");
        return 0;
    }
    println!("最近执行（新→旧）—— snip adopt <序号> \"描述\" 可入库：");
    for (i, e) in entries.iter().enumerate() {
        let mark = if e.saved { "✅已入库" } else { "　存为记录→" };
        println!("[{:>3}] {} {}", i + 1, mark, e.cmd);
        println!("       {}", e.ts);
    }
    0
}

fn cmd_adopt(args: &[String]) -> i32 {
    let Some(idx) = args.first().and_then(|s| s.parse::<usize>().ok()) else {
        eprintln!("用法: snip adopt <序号> \"描述\" [--tag a,b]   （序号见 snip history）");
        return 2;
    };
    let Some(intent) = args.get(1) else {
        eprintln!("缺少描述参数");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let entries = snipnav::history::read_recent(&conn, 300);
    let Some(entry) = (idx >= 1).then(|| entries.get(idx - 1)).flatten() else {
        eprintln!("序号无效（1..={}）", entries.len());
        return 2;
    };
    let mut tags: Vec<String> = Vec::new();
    let mut project: Option<String> = None;
    let mut i = 2;
    while i + 1 < args.len() + 1 && i < args.len() {
        match args[i].as_str() {
            "--tag" | "-t" if i + 1 < args.len() => {
                tags.extend(tagging::split_tags(&args[i + 1]));
                i += 2;
            }
            "--project" | "-p" if i + 1 < args.len() => {
                project = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }
    let project = project.or_else(current_project);
    match snip_add(intent, &entry.cmd, tags, project.as_deref(), None) {
        Ok(id) => {
            println!("已保存 #{id}: {intent}");
            println!("  {}", entry.cmd);
            0
        }
        Err(e) => {
            eprintln!("保存失败: {e}");
            1
        }
    }
}

// ---------- v0.4：团队共享 ----------

fn cmd_export(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("用法: snip export <文件.json>");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let p = snipnav::importer::expand_path(path);
    match snipnav::share::export_self(&conn, &p) {
        Ok(n) => {
            println!("已导出 {n} 条个人记录 → {}", p.display());
            0
        }
        Err(e) => {
            eprintln!("导出失败: {e}");
            1
        }
    }
}

fn cmd_import_share(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("用法: snip import-share <文件.json>");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let p = snipnav::importer::expand_path(path);
    match snipnav::share::import_share(&conn, &p) {
        Ok(st) => {
            println!("导入完成：共 {} 条，新增 {}，合并 {}（同命令保留较大使用量并合并标签）", st.total, st.added, st.merged);
            0
        }
        Err(e) => {
            eprintln!("导入失败: {e}");
            1
        }
    }
}

fn cmd_vacuum() -> i32 {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let path = db::db_path().unwrap_or_default();
    let before = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if let Err(e) = conn.execute_batch("VACUUM;") {
        eprintln!("VACUUM 失败: {e}");
        return 1;
    }
    let after = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!("VACUUM 完成：{} KB → {} KB", before / 1024, after / 1024);
    0
}

fn cmd_delete(args: &[String]) -> i32 {
    let Some(id) = args.first().and_then(|s| s.parse::<i64>().ok()) else {
        eprintln!("用法: snip delete <id>");
        return 2;
    };
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return 1;
        }
    };
    let _ = conn.execute("DELETE FROM interactions WHERE snippet_id = ?1", [id]);
    match conn.execute("DELETE FROM snippets WHERE id = ?1", [id]) {
        Ok(0) => {
            eprintln!("记录不存在: #{id}");
            1
        }
        Ok(_) => {
            println!("已删除 #{id}");
            0
        }
        Err(e) => {
            eprintln!("删除失败: {e}");
            1
        }
    }
}
