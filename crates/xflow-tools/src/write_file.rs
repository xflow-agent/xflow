//! write_file 工具实现

use super::tool::{
    ResultDisplayType, Tool, ToolCategory, ToolConfirmationRequest, ToolDisplayConfig, ToolMetadata,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};
use xflow_model::format_io_error;

/// 敏感路径前缀列表（禁止写入）
const SENSITIVE_WRITE_PATHS: &[&str] = &[
    "/etc/passwd",
    "/etc/shadow",
    "/etc/sudoers",
    "/etc/ssh",
    "/root/.ssh",
    "/root/.bashrc",
    "/root/.profile",
    "/boot",
    "/sys",
    "/proc",
];

/// 最大文件大小限制（10MB）
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// 检查路径是否为敏感路径
fn is_sensitive_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    for sensitive in SENSITIVE_WRITE_PATHS {
        if path_str.starts_with(sensitive) || path_str == *sensitive {
            return true;
        }
    }
    false
}

/// 检查路径是否包含目录遍历攻击
fn has_path_traversal(path: &str) -> bool {
    path.contains("..") || path.contains("~")
}

/// write_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct WriteFileArgs {
    /// 文件路径
    pub path: String,
    /// 文件内容
    pub content: String,
}

/// write_file 工具
pub struct WriteFileTool;

impl WriteFileTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "write_file",
            description:
                "写入文件内容。参数: path - 文件路径, content - 要写入的内容。会覆盖已存在的文件。",
            category: ToolCategory::File,
            danger_level: 1,
            display: ToolDisplayConfig {
                primary_param: "path",
                result_display: ResultDisplayType::StatusOnly,
                max_preview_lines: 5,
                max_preview_chars: 200,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn build_confirmation(&self, args: &serde_json::Value) -> Option<ToolConfirmationRequest> {
        let path = args.get("path")?.as_str()?;
        let content = args.get("content")?.as_str()?;

        // 检查文件是否已存在
        let path_buf = PathBuf::from(path);
        let exists = path_buf.exists();

        // 生成内容预览（前 200 字符）
        let preview = if content.len() > 200 {
            format!("{}...", &content[..200])
        } else {
            content.to_string()
        };

        // 计算行数
        let lines = content.lines().count();

        let message = if exists {
            format!(
                "路径: {}\n操作: 覆盖已有文件\n行数: {}\n内容预览:\n{}",
                path, lines, preview
            )
        } else {
            format!(
                "路径: {}\n操作: 创建新文件\n行数: {}\n内容预览:\n{}",
                path, lines, preview
            )
        };

        let mut req = ToolConfirmationRequest::new(message);

        // 覆盖文件危险等级更高
        if exists {
            req = req.with_danger(2, "将覆盖已有文件内容");
        } else {
            req = req.with_danger(1, "创建新文件");
        }

        Some(req)
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: WriteFileArgs = serde_json::from_value(args)?;

        debug!("写入文件: {}", params.path);

        // 安全检查：目录遍历攻击
        if has_path_traversal(&params.path) {
            warn!("Path traversal detected: {}", params.path);
            return Ok(format!("Error: invalid path: {}", params.path));
        }

        // 规范化路径
        let path = if PathBuf::from(&params.path).is_absolute() {
            PathBuf::from(&params.path)
        } else {
            workdir.join(&params.path)
        };

        debug!("规范化后路径: {:?}", path);

        // 安全检查：敏感路径
        if is_sensitive_path(&path) {
            warn!("Attempt to write sensitive path: {:?}", path);
            return Ok(format!("Error: access denied for sensitive path: {}", params.path));
        }

        if params.content.len() > MAX_FILE_SIZE {
            warn!("File content exceeds size limit: {} bytes", params.content.len());
            return Ok(format!(
                "Error: content exceeds max size limit ({} MB)",
                MAX_FILE_SIZE / 1024 / 1024
            ));
        }

        // 检查父目录是否存在，不存在则创建
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
                debug!("创建目录: {:?}", parent);
            }
        }

        // 写入文件
        match tokio::fs::write(&path, &params.content).await {
            Ok(_) => {
                let lines = params.content.lines().count();
                Ok(format!(
                    "Written: {} ({} lines, {} bytes)",
                    params.path,
                    lines,
                    params.content.len()
                ))
            }
            Err(e) => {
                warn!("Failed to write file: {:?}", e);
                Ok(format_io_error(&e))
            }
        }
    }
}
