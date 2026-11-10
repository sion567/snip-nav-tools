//! 自动打标签 + 从命令文本推导 tool/command。

pub const RULES: &[(&str, &[&str])] = &[
    ("docker", &["docker "]),
    ("k8s", &["kubectl"]),
    ("gradle", &["gradle", "gradlew"]),
    ("maven", &["mvn "]),
    ("sql", &["sqlite3", "mysql", "psql"]),
    ("git", &["git "]),
    (
        "windows",
        &["robocopy", "taskkill", "netsh", "powershell", "netstat", "findstr", "icacls"],
    ),
    (
        "linux",
        &["grep", "awk", "sed", "fuser", "lsof", "chmod", "chown"],
    ),
    ("log", &["log", "logs"]),
    ("filter", &["grep", "awk", "findstr", "|"]),
    ("deploy", &["deploy", "release", "publish"]),
    ("build", &["build", "compile", "package"]),
    ("run", &["run", "start", "bootrun", " up"]),
    ("network", &["port", "netstat", "curl", "wget"]),
    ("permission", &["chmod", "chown", "acl", "icacls"]),
];

/// 规则打标签：命令文本 + 人话描述里含关键词即命中。
pub fn auto_tags(cmd: &str, intent: &str) -> Vec<String> {
    let hay = format!("{cmd} {intent}").to_lowercase();
    RULES
        .iter()
        .filter(|(_, kws)| kws.iter().any(|k| hay.contains(k)))
        .map(|(tag, _)| tag.to_string())
        .collect()
}

/// 人工标签与自动标签合并去重（小写）。
pub fn merge_tags(manual: Option<&[String]>, cmd: &str, intent: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |t: String| {
        let t = t.trim().to_lowercase();
        if !t.is_empty() && seen.insert(t.clone()) {
            out.push(t);
        }
    };
    if let Some(tags) = manual {
        for t in tags {
            push(t.clone());
        }
    }
    for t in auto_tags(cmd, intent) {
        push(t);
    }
    out
}

/// CLI / 表单里的标签串（逗号/中文逗号/空白分隔）。
pub fn split_tags(raw: &str) -> Vec<String> {
    raw.split([',', '，', ' ', '\t'])
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 从命令文本推导 (tool, command)：取第一个管道段的前两个非参数词。
/// ./gradlew bootRun → (gradle, bootRun)；netstat -ano → (netstat, None)
pub fn derive_tool_command(cmd: &str) -> (Option<String>, Option<String>) {
    let head = cmd.split('|').next().unwrap_or("").trim();
    let toks: Vec<&str> = head.split_whitespace().collect();
    if toks.is_empty() {
        return (None, None);
    }
    let tool = match toks[0] {
        "./gradlew" | "gradlew" => "gradle".to_string(),
        t => t.to_string(),
    };
    let command = if toks.len() > 1 {
        let t1 = toks[1];
        let looks_like_arg = t1.starts_with('-')
            || t1.starts_with('/')
            || t1.starts_with('\\')
            || t1.ends_with(".sh")
            || t1.ends_with(".py")
            || t1.ends_with(".js");
        if looks_like_arg {
            None
        } else {
            Some(t1.to_string())
        }
    } else {
        None
    };
    (Some(tool), command)
}

// ---------- v1.8：拼音搜索辅助 ----------

/// 单字符转拼音：汉字返回全拼（无声调小写，ü 归一为 v 的输入习惯），非汉字返回 None。
fn char_pinyin(c: char) -> Option<String> {
    use pinyin::ToPinyin;
    c.to_pinyin().map(|p| p.plain().replace('ü', "v"))
}

/// 意图的拼音形式（无分隔小写）：ASCII 原样保留，汉字转全拼，空白丢弃。
/// "docker 日志" → "dockerrizhi"；用于搜 "rizhi" 这类全拼关键词。
pub fn pinyin_full(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match char_pinyin(c) {
            Some(py) => out.push_str(&py),
            None if !c.is_whitespace() => out.extend(c.to_lowercase()),
            None => {}
        }
    }
    out
}

/// 意图的首字母形式：汉字取拼音首字母，ASCII 原样保留，空白丢弃。
/// "docker 日志" → "dockerrz"；用于搜 "rz" 这类缩写关键词。
pub fn pinyin_initials(text: &str) -> String {
    let mut out = String::with_capacity(text.len() / 2);
    for c in text.chars() {
        match char_pinyin(c) {
            Some(py) => {
                if let Some(first) = py.chars().next() {
                    out.push(first);
                }
            }
            None if !c.is_whitespace() => out.extend(c.to_lowercase()),
            None => {}
        }
    }
    out
}
/// 返回简短原因；规则按子串匹配（小写），更具体的放前面。
/// 读记录时即时计算（不落库），规则改进后老记录自动跟上。
pub fn danger_reason(cmd: &str) -> Option<&'static str> {
    const RULES: &[(&str, &str)] = &[
        // 删文件 / 删数据
        ("rm -rf", "递归强删，不可恢复"),
        ("rm -fr", "递归强删，不可恢复"),
        ("rm -r -f", "递归强删，不可恢复"),
        ("del /f", "强制删除文件"),
        ("del /s", "递归删除文件"),
        ("rd /s", "递归删除目录"),
        ("remove-item -recurse", "递归删除"),
        // Git 不可逆操作
        ("git reset --hard", "丢弃全部未提交修改"),
        ("git push --force", "强制覆盖远端历史"),
        ("git push -f", "强制覆盖远端历史"),
        ("git clean -f", "删除未跟踪文件"),
        // 数据库
        ("drop database", "删库"),
        ("drop table", "删表"),
        ("truncate table", "清空表数据"),
        ("shred ", "覆写销毁文件"),
        // 权限 / 系统
        ("chmod -r 777", "全开放权限"),
        ("chmod 777", "全开放权限"),
        ("shutdown", "关机"),
        ("restart-computer", "重启系统"),
        ("mkfs", "格式化文件系统"),
        ("format c:", "格式化磁盘"),
        ("format d:", "格式化磁盘"),
        ("> /dev/sd", "写裸设备"),
        // 容器 / K8s 清理
        ("docker system prune -a", "删除全部未使用镜像"),
        ("docker system prune", "清理未用容器/网络/缓存"),
        ("docker volume prune", "删除未用数据卷"),
        ("kubectl delete", "删除 K8s 资源"),
    ];
    let c = cmd.to_lowercase();
    RULES
        .iter()
        .find(|(pat, _)| c.contains(pat))
        .map(|(_, why)| *why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_tags_docker_log() {
        let tags = auto_tags("docker logs -f web | grep ERROR", "看容器错误日志");
        assert!(tags.contains(&"docker".to_string()));
        assert!(tags.contains(&"log".to_string()));
        assert!(tags.contains(&"filter".to_string()));
    }

    #[test]
    fn auto_tags_windows() {
        let tags = auto_tags("netstat -ano | findstr :8080", "查端口占用");
        assert!(tags.contains(&"windows".to_string()));
        assert!(tags.contains(&"network".to_string()));
    }

    #[test]
    fn derive_tools() {
        assert_eq!(
            derive_tool_command("docker logs -f web"),
            (Some("docker".into()), Some("logs".into()))
        );
        assert_eq!(
            derive_tool_command("./gradlew bootRun --args=x"),
            (Some("gradle".into()), Some("bootRun".into()))
        );
        assert_eq!(
            derive_tool_command("netstat -ano"),
            (Some("netstat".into()), None)
        );
        assert_eq!(
            derive_tool_command("grep -C 5 ERROR a.log | head"),
            (Some("grep".into()), None)
        );
    }

    #[test]
    fn merge_keeps_manual_first() {
        let manual = vec!["Docker".to_string(), "部署".to_string()];
        let tags = merge_tags(Some(&manual), "docker ps", "");
        assert_eq!(tags[0], "docker");
        assert_eq!(tags[1], "部署");
    }

    #[test]
    fn danger_rules_match() {
        assert_eq!(danger_reason("rm -rf /tmp/x"), Some("递归强删，不可恢复"));
        assert_eq!(danger_reason("git reset --hard HEAD~1"), Some("丢弃全部未提交修改"));
        assert_eq!(danger_reason("DROP TABLE users;"), Some("删表"), "大小写不敏感");
        assert_eq!(
            danger_reason("docker system prune -af"),
            Some("删除全部未使用镜像"),
            "更具体的规则优先"
        );
        assert_eq!(danger_reason("git clean -fdx"), Some("删除未跟踪文件"));
    }

    #[test]
    fn danger_rules_no_false_positive() {
        assert_eq!(danger_reason("docker ps -a"), None);
        assert_eq!(danger_reason("git log --oneline"), None);
        assert_eq!(danger_reason("npm run format"), None, "format 单词不触发磁盘格式化");
        assert_eq!(danger_reason("git push origin main"), None);
        assert_eq!(danger_reason("rm -i file.txt"), None, "交互式删除不标危险");
    }

    #[test]
    fn pinyin_conversions() {
        assert_eq!(pinyin_full("日志"), "rizhi");
        assert_eq!(pinyin_full("日志过滤"), "rizhiguolv");
        assert_eq!(pinyin_full("docker 日志"), "dockerrizhi", "ASCII 原样、空白丢弃");
        assert_eq!(pinyin_initials("日志过滤"), "rzgl");
        assert_eq!(pinyin_initials("docker 日志"), "dockerrz");
        assert_eq!(pinyin_full("gradle 启动"), "gradleqidong");
        assert_eq!(pinyin_initials("gradle 启动"), "gradleqd");
        // 多音字取最常见读音（了 → le 而非 liao；多音字精确匹配不做，够用）
        assert_eq!(pinyin_full("了解"), "lejie");
    }
}
