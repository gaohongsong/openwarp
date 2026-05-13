//! 简单的 `~/.ssh/config` 解析器 — 行式状态机,只提取我们关心的指令。
//!
//! 不依赖外部 crate(ssh2 之类),也不尝试覆盖 OpenSSH 的全部指令。
//! `Host *` 通配符块被跳过:无法映射到单个 SshServerInfo。

use anyhow::{Context, Result};
use std::path::PathBuf;

/// 从 `~/.ssh/config` 解析出的单个 Host 块。
#[derive(Clone, Debug, Default)]
pub struct SshConfigHost {
    pub alias: String,
    pub host_name: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    pub connect_timeout_secs: Option<u32>,
    pub server_alive_interval_secs: Option<u32>,
    pub server_alive_count_max: Option<u32>,
    pub host_key_algorithms: Option<String>,
    pub pubkey_accepted_key_types: Option<String>,
}

/// 解析 ssh config 文本,返回所有非通配符的 Host 块。
///
/// - 空行和 `#` 注释被忽略
/// - `Host *` 通配符块跳过
/// - 缺少 `HostName` 时用 alias 作为 hostname
/// - 重复指令:后出现的覆盖先出现的
pub fn parse_ssh_config(config_text: &str) -> Vec<SshConfigHost> {
    let mut hosts: Vec<SshConfigHost> = Vec::new();
    let mut current: Option<SshConfigHost> = None;

    for raw_line in config_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 分割 "Keyword Value"。SSH config 允许 `=` 分隔或空格分隔。
        let (keyword, value) = match split_directive(line) {
            Some(pair) => pair,
            None => continue,
        };

        match keyword {
            "Host" => {
                if let Some(host) = current.take() {
                    hosts.push(finalize_host(host));
                }
                // 跳过通配符 Host *
                if value.contains('*') || value.contains('?') {
                    current = None;
                } else {
                    current = Some(SshConfigHost {
                        alias: value.to_string(),
                        ..Default::default()
                    });
                }
            }
            _ => {
                if let Some(ref mut host) = current {
                    apply_directive(host, keyword, value);
                }
                // 不在 Host 块内的指令(全局默认)被忽略
            }
        }
    }

    if let Some(host) = current.take() {
        hosts.push(finalize_host(host));
    }

    hosts
}

/// 读取 `~/.ssh/config` 文件内容。文件不存在时返回空字符串(而非报错),
/// 让上层统一处理"无 Host 可导入"的场景。
pub fn read_ssh_config_file() -> Result<String> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let path = home.join(".ssh").join("config");
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))
}

/// 把完成的 Host 块做最终修饰:没有 HostName 时用 alias。
fn finalize_host(mut host: SshConfigHost) -> SshConfigHost {
    if host.host_name.is_none() {
        host.host_name = Some(host.alias.clone());
    }
    host
}

/// 分割 "Keyword Value" — 处理 `=` 分隔和空格分隔两种形式。
fn split_directive(line: &str) -> Option<(&str, &str)> {
    // 先尝试按 `=` 分割(SSH config 支持这种语法)
    if let Some(eq_pos) = line.find('=') {
        let kw = line[..eq_pos].trim();
        let val = line[eq_pos + 1..].trim();
        if kw.is_empty() || val.is_empty() {
            return None;
        }
        return Some((kw, val));
    }
    // 再按第一个空格分割
    let mut iter = line.splitn(2, char::is_whitespace);
    let kw = iter.next()?.trim();
    let val = iter.next()?.trim();
    if kw.is_empty() || val.is_empty() {
        return None;
    }
    Some((kw, val))
}

/// 把解析出的指令值写入 Host 结构体。
fn apply_directive(host: &mut SshConfigHost, keyword: &str, value: &str) {
    match keyword {
        "HostName" => host.host_name = Some(value.to_string()),
        "Port" => host.port = value.parse().ok(),
        "User" => host.user = Some(value.to_string()),
        "IdentityFile" => host.identity_file = Some(value.to_string()),
        "ProxyJump" => host.proxy_jump = Some(value.to_string()),
        "ConnectTimeout" => host.connect_timeout_secs = value.parse().ok(),
        "ServerAliveInterval" => host.server_alive_interval_secs = value.parse().ok(),
        "ServerAliveCountMax" => host.server_alive_count_max = value.parse().ok(),
        "HostKeyAlgorithms" => host.host_key_algorithms = Some(value.to_string()),
        "PubkeyAcceptedKeyTypes" => host.pubkey_accepted_key_types = Some(value.to_string()),
        _ => {} // 忽略不关心的指令
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_typical_config() {
        let config = "\
Host bastion
    HostName bastion.example.com
    Port 2222
    User admin
    IdentityFile ~/.ssh/id_bastion
    ConnectTimeout 10

Host web1
    HostName web1.internal
    User deploy
    ProxyJump bastion
    ServerAliveInterval 60
    ServerAliveCountMax 3";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts.len(), 2);

        assert_eq!(hosts[0].alias, "bastion");
        assert_eq!(hosts[0].host_name.as_deref(), Some("bastion.example.com"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[0].user.as_deref(), Some("admin"));
        assert_eq!(hosts[0].connect_timeout_secs, Some(10));

        assert_eq!(hosts[1].alias, "web1");
        assert_eq!(hosts[1].proxy_jump.as_deref(), Some("bastion"));
        assert_eq!(hosts[1].server_alive_interval_secs, Some(60));
        assert_eq!(hosts[1].server_alive_count_max, Some(3));
    }

    #[test]
    fn wildcard_host_excluded() {
        let config = "\
Host *
    ServerAliveInterval 120

Host myserver
    HostName 10.0.0.1";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "myserver");
    }

    #[test]
    fn hostname_defaults_to_alias() {
        let config = "Host shortcut\n    User root";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts[0].host_name.as_deref(), Some("shortcut"));
    }

    #[test]
    fn duplicate_directive_last_wins() {
        let config = "\
Host srv
    Port 22
    Port 2222";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts[0].port, Some(2222));
    }

    #[test]
    fn comments_and_blanks_ignored() {
        let config = "\
# This is a comment

Host srv
    # another comment
    HostName example.com";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host_name.as_deref(), Some("example.com"));
    }

    #[test]
    fn empty_file() {
        let hosts = parse_ssh_config("");
        assert!(hosts.is_empty());
    }

    #[test]
    fn equals_syntax() {
        let config = "Host srv\n    User=root\n    Port=2222";
        let hosts = parse_ssh_config(config);
        assert_eq!(hosts[0].user.as_deref(), Some("root"));
        assert_eq!(hosts[0].port, Some(2222));
    }
}
