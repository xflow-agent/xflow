//! read_file 工具实现

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

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
            description: "读取文件内容。参数: path - 要读取的文件路径（绝对路径或相对路径）",
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
                    "description": "要读取的文件路径"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: ReadFileArgs = serde_json::from_value(args)?;

        debug!("读取文件: {}", params.path);

        // 获取当前工作目录
        let workdir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // 规范化并验证路径
        let path = match normalize_and_validate_path(&params.path, &workdir) {
            Ok(p) => p,
            Err(e) => {
                warn!("路径验证失败: {} - {}", params.path, e);
                return Ok(format!("错误: {}", e));
            }
        };

        // 安全检查：敏感路径
        if is_sensitive_path(&path) {
            warn!("尝试访问敏感路径: {:?}", path);
            return Ok(format!("错误: 拒绝访问敏感路径: {}", params.path));
        }

        // 检查文件是否存在
        if !path.exists() {
            warn!("文件不存在: {:?}", path);
            return Ok(format!("错误: 文件不存在: {}", params.path));
        }

        // 检查是否为文件
        if !path.is_file() {
            return Ok(format!("错误: {} 不是文件", params.path));
        }

        // 读取文件内容
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let lines = content.lines().count();
                Ok(format!(
                    "文件: {} ({} 行)\n---\n{}\n---",
                    params.path, lines, content
                ))
            }
            Err(e) => {
                warn!("读取文件失败: {:?}", e);
                // 清理错误信息，避免泄露内部路径
                let safe_error = match e.kind() {
                    std::io::ErrorKind::NotFound => "文件不存在".to_string(),
                    std::io::ErrorKind::PermissionDenied => "权限不足".to_string(),
                    _ => "读取失败".to_string(),
                };
                Ok(format!("错误: {}", safe_error))
            }
        }
    }
}
