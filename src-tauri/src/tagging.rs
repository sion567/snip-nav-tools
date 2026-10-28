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
}
