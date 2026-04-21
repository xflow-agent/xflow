//! read_file 工具实现

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};
use xflow_model::format_io_error;

/// 敏感路径前缀列表（禁止访问）
const SENSITIVE_PATHS: &[&str] = &[
    "/etc/passwd",
    "/etc/shadow",
    "/etc/sudoers",
    "/root/.ssh",
    "/root/.gnupg",
    "/var/log/auth.log",
    "/proc/self/environ",
];

/// 检查路径是否为敏感路径
fn is_sensitive_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    for sensitive in SENSITIVE_PATHS {
        if path_str.starts_with(sensitive) || path_str == *sensitive {
            return true;
        }
    }
    false
}

/// 检查路径是否包含目录遍历攻击
fn has_path_traversal(path: &str) -> bool {
    // 解码 URL 编码
    let decoded = path
        .replace("%2e", ".")
        .replace("%2f", "/")
        .replace("%5c", "\\");

    // 检查各种遍历模式
    decoded.contains("..") 
        || decoded.contains("~")
        || decoded.starts_with("/")
        || decoded.starts_with("\\")
        || decoded.contains(":/")  // Windows 绝对路径
        || decoded.contains(":\\")
}

/// 规范化并验证路径
fn normalize_and_validate_path(
    path_str: &str,
    workdir: &std::path::Path,
) -> Result<PathBuf, String> {
    // 首先检查明显的遍历攻击
    if has_path_traversal(path_str) {
        return Err("路径包含非法字符".to_string());
    }

    let path = PathBuf::from(path_str);

    // 如果路径是绝对路径，检查是否在允许范围内
    if path.is_absolute() {
        // 检查是否是敏感路径
        if is_sensitive_path(&path) {
            return Err("无法访问系统敏感路径".to_string());
        }
        Ok(path)
    } else {
        // 相对路径，与工作目录拼接
        let full_path = workdir.join(&path);
        // 规范化路径（解析 .. 和 .）
        match full_path.canonicalize() {
            Ok(canonical) => {
                // 确保规范化后的路径仍在允许的范围内
                // 这里可以添加额外的检查，如确保路径在工作目录下
                Ok(canonical)
            }
            Err(_) => {
                // 文件可能不存在，返回拼接后的路径
                Ok(full_path)
            }
        }
    }
}

/// read_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadFileArgs {
    /// 文件路径
    pub path: String,
}

/// read_file 工具
pub struct ReadFileTool;

impl ReadFileTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "read_file",
            description: "Read file contents. Param: path - file path to read (absolute or relative)",
            category: ToolCategory::File,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "path",
                result_display: ResultDisplayType::LineCount,
                max_preview_lines: 10,
                max_preview_chars: 500,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: ReadFileArgs = serde_json::from_value(args)?;

        debug!("读取文件: {}", params.path);

        // 规范化并验证路径
        let path = match normalize_and_validate_path(&params.path, workdir) {
            Ok(p) => p,
            Err(e) => {
                warn!("Path validation failed: {} - {}", params.path, e);
                return Ok(format!("Error: {}", e));
            }
        };

        if is_sensitive_path(&path) {
            warn!("Attempt to access sensitive path: {:?}", path);
            return Ok(format!("Error: access denied for sensitive path: {}", params.path));
        }

        if !path.exists() {
            return Ok(format!("Error: file not found: {}", params.path));
        }

        if !path.is_file() {
            return Ok(format!("Error: {} is not a file", params.path));
        }

        // 读取文件内容
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let lines = content.lines().count();
                let bytes = content.len();

                if content.len() > 5000 {
                    let preview_lines: Vec<&str> = content.lines().take(100).collect();
                    Ok(format!(
                        "File: {} ({} lines, {} bytes)\n---\n{}\n...\n[{} more lines]",
                        params.path, lines, bytes,
                        preview_lines.join("\n"),
                        lines.saturating_sub(100)
                    ))
                } else {
                    Ok(format!(
                        "File: {} ({} lines, {} bytes)\n---\n{}\n---",
                        params.path, lines, bytes, content
                    ))
                }
            }
            Err(e) => {
                warn!("Failed to read file: {:?}", e);
                Ok(format_io_error(&e))
            }
        }
    }
}
