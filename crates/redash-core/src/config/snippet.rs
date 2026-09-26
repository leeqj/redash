use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandSnippet {
    pub id: String,
    pub name: String,
    pub category: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnippetLibrary {
    pub snippets: Vec<CommandSnippet>,
}

impl Default for SnippetLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SnippetLibrary {
    pub fn new() -> Self {
        Self {
            snippets: Self::default_presets(),
        }
    }

    pub fn empty() -> Self {
        Self {
            snippets: Vec::new(),
        }
    }

    pub fn default_presets() -> Vec<CommandSnippet> {
        vec![
            // System
            CommandSnippet {
                id: "sys-version".into(),
                name: "查看系统版本".into(),
                category: "System".into(),
                command: "cat /etc/os-release 2>/dev/null || uname -a".into(),
                description: "输出操作系统发行版信息与内核版本".into(),
            },
            CommandSnippet {
                id: "sys-large-files".into(),
                name: "查看磁盘大文件 (Top 10)".into(),
                category: "System".into(),
                command: "du -ahx / 2>/dev/null | sort -rh | head -n 10".into(),
                description: "扫描根目录下最大的 10 个文件或目录".into(),
            },
            CommandSnippet {
                id: "sys-clean-journal".into(),
                name: "清理 Journal 日志".into(),
                category: "System".into(),
                command: "journalctl --vacuum-time=3d 2>/dev/null || journalctl --vacuum-size=500M 2>/dev/null".into(),
                description: "清理超过 3 天或占用大于 500M 的 systemd 日志".into(),
            },
            // Docker
            CommandSnippet {
                id: "docker-stats".into(),
                name: "查看容器资源占用".into(),
                category: "Docker".into(),
                command: "docker stats --no-stream".into(),
                description: "一次性输出所有正在运行的容器 CPU、内存与网络使用率".into(),
            },
            CommandSnippet {
                id: "docker-prune".into(),
                name: "清理无用容器与镜像".into(),
                category: "Docker".into(),
                command: "docker system prune -af --volumes".into(),
                description: "彻底清理未使用的容器、镜像、网络与数据卷".into(),
            },
            CommandSnippet {
                id: "docker-status".into(),
                name: "查看 Docker 守护进程状态".into(),
                category: "Docker".into(),
                command: "systemctl status docker --no-pager 2>/dev/null || service docker status".into(),
                description: "查看 Docker systemd 服务运行状态".into(),
            },
            // Network
            CommandSnippet {
                id: "net-ping-cf".into(),
                name: "测试外部连通性 (Ping Cloudflare)".into(),
                category: "Network".into(),
                command: "ping -c 4 1.1.1.1".into(),
                description: "发送 4 个 ICMP 数据包至 Cloudflare DNS 测试网络连通性与延迟".into(),
            },
            CommandSnippet {
                id: "net-conn-stats".into(),
                name: "查看对外连接统计".into(),
                category: "Network".into(),
                command: "ss -s 2>/dev/null || netstat -s 2>/dev/null".into(),
                description: "显示当前系统的 TCP/UDP 传输层连接汇总状态".into(),
            },
            CommandSnippet {
                id: "net-speedtest".into(),
                name: "测试当前网络测速".into(),
                category: "Network".into(),
                command: "curl -s https://raw.githubusercontent.com/sivel/speedtest-cli/master/speedtest.py | python3 - 2>/dev/null || speedtest-cli 2>/dev/null".into(),
                description: "运行 Python speedtest-cli 进行下行与上行带宽测速".into(),
            },
            // Maintenance
            CommandSnippet {
                id: "maint-pkg-update".into(),
                name: "一键更新系统软件包".into(),
                category: "Maintenance".into(),
                command: "apt update && apt upgrade -y 2>/dev/null || yum update -y 2>/dev/null || dnf update -y 2>/dev/null".into(),
                description: "适配 Debian/Ubuntu 与 RHEL/CentOS/Fedora 的自动化系统包更新".into(),
            },
            CommandSnippet {
                id: "maint-crash-logs".into(),
                name: "查看重启记录与崩溃日志".into(),
                category: "Maintenance".into(),
                command: "last reboot | head -n 10; dmesg -T --level=err,warn 2>/dev/null | tail -n 20".into(),
                description: "查看最近 10 次系统重启历史以及内核最新警告与错误".into(),
            },
        ]
    }

    pub fn add(&mut self, snippet: CommandSnippet) {
        if let Some(pos) = self.snippets.iter().position(|s| s.id == snippet.id) {
            self.snippets[pos] = snippet;
        } else {
            self.snippets.push(snippet);
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<CommandSnippet> {
        if let Some(pos) = self.snippets.iter().position(|s| s.id == id) {
            Some(self.snippets.remove(pos))
        } else {
            None
        }
    }

    pub fn get(&self, id: &str) -> Option<&CommandSnippet> {
        self.snippets.iter().find(|s| s.id == id)
    }

    pub fn list(&self) -> &[CommandSnippet] {
        &self.snippets
    }

    pub fn list_by_category(&self, category: &str) -> Vec<&CommandSnippet> {
        self.snippets
            .iter()
            .filter(|s| s.category.eq_ignore_ascii_case(category))
            .collect()
    }

    pub fn categories(&self) -> Vec<String> {
        let mut cats = Vec::new();
        for s in &self.snippets {
            if !cats.contains(&s.category) {
                cats.push(s.category.clone());
            }
        }
        cats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snippet_library_defaults() {
        let lib = SnippetLibrary::new();
        assert_eq!(lib.list().len(), 11);

        let system_snippets = lib.list_by_category("System");
        assert_eq!(system_snippets.len(), 3);
        assert!(system_snippets.iter().any(|s| s.name == "查看系统版本"));
        assert!(
            system_snippets
                .iter()
                .any(|s| s.name == "查看磁盘大文件 (Top 10)")
        );
        assert!(
            system_snippets
                .iter()
                .any(|s| s.name == "清理 Journal 日志")
        );

        let docker_snippets = lib.list_by_category("Docker");
        assert_eq!(docker_snippets.len(), 3);
        assert!(docker_snippets.iter().any(|s| s.name == "查看容器资源占用"));
        assert!(
            docker_snippets
                .iter()
                .any(|s| s.name == "清理无用容器与镜像")
        );
        assert!(
            docker_snippets
                .iter()
                .any(|s| s.name == "查看 Docker 守护进程状态")
        );

        let net_snippets = lib.list_by_category("Network");
        assert_eq!(net_snippets.len(), 3);
        assert!(
            net_snippets
                .iter()
                .any(|s| s.name == "测试外部连通性 (Ping Cloudflare)")
        );
        assert!(net_snippets.iter().any(|s| s.name == "查看对外连接统计"));
        assert!(net_snippets.iter().any(|s| s.name == "测试当前网络测速"));

        let maint_snippets = lib.list_by_category("Maintenance");
        assert_eq!(maint_snippets.len(), 2);
        assert!(
            maint_snippets
                .iter()
                .any(|s| s.name == "一键更新系统软件包")
        );
        assert!(
            maint_snippets
                .iter()
                .any(|s| s.name == "查看重启记录与崩溃日志")
        );

        let categories = lib.categories();
        assert_eq!(
            categories,
            vec!["System", "Docker", "Network", "Maintenance"]
        );
    }

    #[test]
    fn test_snippet_crud_and_serde() {
        let mut lib = SnippetLibrary::empty();
        assert_eq!(lib.list().len(), 0);

        let custom = CommandSnippet {
            id: "custom-1".into(),
            name: "Tail Nginx Access".into(),
            category: "Custom".into(),
            command: "tail -f /var/log/nginx/access.log".into(),
            description: "实时监控 Nginx 访问日志".into(),
        };

        lib.add(custom.clone());
        assert_eq!(lib.list().len(), 1);
        assert_eq!(lib.get("custom-1"), Some(&custom));

        // Update
        let mut updated = custom.clone();
        updated.command = "tail -n 100 -f /var/log/nginx/access.log".into();
        lib.add(updated.clone());
        assert_eq!(lib.list().len(), 1);
        assert_eq!(
            lib.get("custom-1").unwrap().command,
            "tail -n 100 -f /var/log/nginx/access.log"
        );

        // Serde
        let json = serde_json::to_string(&lib).expect("serialize");
        let deserialized: SnippetLibrary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(lib, deserialized);

        // Remove
        let removed = lib.remove("custom-1");
        assert_eq!(removed, Some(updated));
        assert_eq!(lib.list().len(), 0);
    }
}
