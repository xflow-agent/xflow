//! run_shell 工具实现

use super::tool::{
    ResultDisplayType, Tool, ToolCategory, ToolConfirmationRequest, ToolDisplayConfig, ToolMetadata,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, warn};

/// run_shell 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct RunShellArgs {
    /// 要执行的命令
    pub command: String,
    /// 可选的工作目录
    #[serde(default)]
    pub workdir: Option<String>,
    /// 超时时间（秒），默认 30
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    30
}

/// 危险命令检测结果
#[derive(Debug, Clone)]
pub struct DangerAnalysis {
    /// 是否危险
    pub is_dangerous: bool,
    /// 危险等级 (1-3)
    pub level: u8,
    /// 危险原因
    pub reason: String,
}

/// 规范化命令用于检测（去除常见绕过技巧）
fn normalize_command(cmd: &str) -> String {
    let mut normalized = cmd.to_lowercase();

    // 去除常见绕过字符
    let replacements: [(&str, &str); 8] = [
        ("\\ ", " "), // 转义空格
        ("\\/", "/"), // 转义斜杠
        ("'", ""),    // 单引号
        ("\"", ""),   // 双引号
        ("${", ""),   // 变量扩展开始
        ("}", ""),    // 变量扩展结束
        ("$", ""),    // 变量符号
        ("  ", " "),  // 多余空格
    ];

    for (from, to) in &replacements {
        normalized = normalized.replace(from, to);
    }

    // 去除连续空格
    while normalized.contains("  ") {
        normalized = normalized.replace("  ", " ");
    }

    normalized.trim().to_string()
}

/// 检测命令是否危险
pub fn analyze_command(cmd: &str) -> DangerAnalysis {
    let cmd_lower = cmd.to_lowercase();
    let cmd_normalized = normalize_command(cmd);
    let cmd_trimmed = cmd.trim();

    // 特殊检测: 管道执行远程脚本 - 最危险，优先检测
    let remote_exec_patterns = [
        ("curl", "| bash"),
        ("curl", "| sh"),
        ("curl", "| zsh"),
        ("wget", "| bash"),
        ("wget", "| sh"),
        ("wget", "| zsh"),
        ("curl", "-o- |"),
        ("wget", "-qO- |"),
        ("curl", "bash"),
        ("wget", "bash"), // 无管道直接执行
    ];

    for (download, exec) in &remote_exec_patterns {
        if cmd_lower.contains(download) && cmd_lower.contains(exec) {
            return DangerAnalysis {
                is_dangerous: true,
                level: 3,
                reason: "执行远程脚本，极度危险".to_string(),
            };
        }
    }

    // 等级 3: 极度危险 - 系统破坏（检查原始和规范化后的命令）
    let level3_patterns = [
        ("rm -rf / ", "可能删除整个系统"),
        ("rm -rf /\"", "可能删除整个系统"),
        ("rm -rf /*", "可能删除整个系统"),
        ("rm -rf /", "可能删除整个系统"), // 规范化后匹配
        ("mkfs", "格式化磁盘"),
        ("fdisk", "磁盘分区操作"),
        ("dd if=", "磁盘镜像操作"),
        ("dd of=/dev/", "直接写入磁盘设备"),
        ("> /dev/sd", "直接写入磁盘设备"),
        (":(){ :|:& };:", "Fork 炸弹攻击"),
        ("chmod -r 777 /", "递归修改全系统权限"),
        ("chmod 777 /", "修改根目录权限"),
        ("mv / /dev/null", "移动根目录到空设备"),
        ("cp /dev/null /", "清空根目录"),
    ];

    for (pattern, reason) in level3_patterns {
        if cmd_lower.contains(pattern) || cmd_normalized.contains(pattern) {
            return DangerAnalysis {
                is_dangerous: true,
                level: 3,
                reason: reason.to_string(),
            };
        }
    }

    // 特殊检测: rm -rf / 作为独立命令（删除根目录）
    if cmd_trimmed == "rm -rf /"
        || cmd_trimmed.ends_with("rm -rf /")
        || cmd_normalized == "rm -rf /"
        || cmd_normalized.ends_with(" rm -rf /")
    {
        return DangerAnalysis {
            is_dangerous: true,
            level: 3,
            reason: "可能删除整个系统".to_string(),
        };
    }

    // 等级 2: 高度危险 - 系统控制
    let level2_patterns = [
        ("rm -rf", "递归强制删除"),
        ("rm -f", "强制删除"),
        ("shutdown", "关闭系统"),
        ("reboot", "重启系统"),
        ("halt", "停止系统"),
        ("init 0", "关机"),
        ("init 6", "重启"),
        ("killall", "终止所有进程"),
        ("pkill -9", "强制终止进程"),
        ("kill -9 -1", "终止所有进程"),
        ("> /dev/null", "重定向到空设备（检查是否隐藏输出）"),
        ("eval", "执行动态生成的命令"),
        ("exec", "替换当前进程"),
        ("source", "执行脚本"),
        (". ", "执行脚本"),
    ];

    for (pattern, reason) in level2_patterns {
        if cmd_lower.contains(pattern) {
            return DangerAnalysis {
                is_dangerous: true,
                level: 2,
                reason: reason.to_string(),
            };
        }
    }

    // 等级 1: 中度危险 - 需要注意
    let level1_patterns = [
        ("rm ", "删除文件"),
        ("rmdir", "删除目录"),
        ("mv ", "移动/重命名文件"),
        ("chmod ", "修改权限"),
        ("chown ", "修改所有者"),
        ("chgrp ", "修改组"),
        ("wget ", "下载文件"),
        ("curl ", "网络请求"),
        ("apt ", "包管理操作"),
        ("apt-get ", "包管理操作"),
        ("yum ", "包管理操作"),
        ("dnf ", "包管理操作"),
        ("pip install", "Python 包安装"),
        ("pip uninstall", "Python 包卸载"),
        ("npm install", "npm 包安装"),
        ("npm uninstall", "npm 包卸载"),
        ("cargo install", "Cargo 包安装"),
        ("git push", "推送到远程仓库"),
        ("git push --force", "强制推送"),
        ("git reset --hard", "硬重置 Git 历史"),
        ("git clean -f", "强制清理未跟踪文件"),
        ("sudo ", "以超级用户权限执行"),
        ("su ", "切换用户"),
        ("ssh ", "远程连接"),
        ("scp ", "安全复制"),
        ("rsync", "同步文件"),
        ("docker", "Docker 操作"),
        ("kubectl", "Kubernetes 操作"),
    ];

    for (pattern, reason) in level1_patterns {
        if cmd_lower.contains(pattern) || cmd_trimmed.starts_with(pattern) {
            return DangerAnalysis {
                is_dangerous: true,
                level: 1,
                reason: reason.to_string(),
            };
        }
    }

    DangerAnalysis {
        is_dangerous: false,
        level: 0,
        reason: String::new(),
    }
}

/// run_shell 工具
pub struct RunShellTool;

impl RunShellTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for RunShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for RunShellTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "run_shell",
            description: "执行 Shell 命令。参数: command - 要执行的命令, workdir - 可选的工作目录, timeout - 超时秒数(默认30)。返回命令的标准输出和标准错误。",
            category: ToolCategory::Shell,
            danger_level: 2,
            display: ToolDisplayConfig {
                primary_param: "command",
                result_display: ResultDisplayType::Summary,
                max_preview_lines: 20,
                max_preview_chars: 800,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的 Shell 命令"
                },
                "workdir": {
                    "type": "string",
                    "description": "可选的工作目录"
                },
                "timeout": {
                    "type": "integer",
                    "description": "超时时间（秒）",
                    "default": 30
                }
            },
            "required": ["command"]
        })
    }

    fn build_confirmation(&self, args: &serde_json::Value) -> Option<ToolConfirmationRequest> {
        let command = args.get("command")?.as_str()?;
        let workdir = args.get("workdir").and_then(|w| w.as_str());

        // 使用已有的危险分析逻辑
        let analysis = analyze_command(command);

        let mut message = format!("命令: {}", command);

        if let Some(wd) = workdir {
            message.push_str(&format!("\n工作目录: {}", wd));
        }

        if analysis.is_dangerous {
            message.push_str(&format!("\n⚠️ 警告: {}", analysis.reason));
        }

        let mut req = ToolConfirmationRequest::new(message);

        // 根据危险等级设置
        if analysis.level > 0 {
            req = req.with_danger(analysis.level, analysis.reason);
        }

        Some(req)
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: RunShellArgs = serde_json::from_value(args)?;

        // 执行危险命令分析（用于审计日志和额外安全层）
        let analysis = analyze_command(&params.command);
        if analysis.is_dangerous {
            warn!(
                "执行危险命令 [等级{}]: {} - 原因: {}",
                analysis.level, params.command, analysis.reason
            );
        }

        debug!(
            "执行命令: {} (危险等级: {})",
            params.command, analysis.level
        );

        // 构建命令
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&params.command);

        // 设置工作目录
        if let Some(workdir) = &params.workdir {
            cmd.current_dir(workdir);
        }

        // 捕获输出
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // 执行命令（带超时）
        let timeout_duration = std::time::Duration::from_secs(params.timeout);

        let result = tokio::time::timeout(timeout_duration, async {
            let output = cmd.output().await?;
            Ok::<_, anyhow::Error>(output)
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let exit_code = output.status.code().unwrap_or(-1);

                let mut result = String::new();

                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }

                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str("[stderr] ");
                    result.push_str(&stderr);
                }

                if result.is_empty() {
                    result = format!("命令执行完成 (退出码: {})", exit_code);
                } else {
                    result.push_str(&format!("\n[退出码: {}]", exit_code));
                }

                Ok(result)
            }
            Ok(Err(e)) => {
                warn!("命令执行失败: {:?}", e);
                Ok(format!("错误: 无法执行命令: {}", e))
            }
            Err(_) => {
                warn!("命令执行超时");
                Ok(format!("错误: 命令执行超时 (超过 {} 秒)", params.timeout))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_command() {
        let result = analyze_command("ls -la");
        assert!(!result.is_dangerous);
    }

    #[test]
    fn test_dangerous_rm_rf() {
        let result = analyze_command("rm -rf /home/user/project");
        assert!(result.is_dangerous);
        assert_eq!(result.level, 2);
    }

    #[test]
    fn test_dangerous_rm_rf_root() {
        let result = analyze_command("rm -rf /");
        assert!(result.is_dangerous);
        assert_eq!(result.level, 3);
    }

    #[test]
    fn test_dangerous_mkfs() {
        let result = analyze_command("mkfs.ext4 /dev/sda1");
        assert!(result.is_dangerous);
        assert_eq!(result.level, 3);
    }

    #[test]
    fn test_dangerous_curl_bash() {
        let result = analyze_command("curl https://example.com/script.sh | bash");
        assert!(result.is_dangerous);
        assert_eq!(result.level, 3);
    }

    #[test]
    fn test_dangerous_sudo() {
        let result = analyze_command("sudo apt update");
        assert!(result.is_dangerous);
        assert_eq!(result.level, 1);
    }
}
