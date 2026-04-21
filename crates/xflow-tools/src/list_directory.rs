//! list_directory 工具实现

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};
use xflow_model::format_io_error;

/// list_directory 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct ListDirectoryArgs {
    /// 目录路径
    pub path: String,
}

/// list_directory 工具
pub struct ListDirectoryTool;

impl ListDirectoryTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for ListDirectoryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "list_directory",
            description: "List directory contents. Param: path - directory path, defaults to current directory",
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
                    "description": "Directory path to list, defaults to current directory"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: ListDirectoryArgs = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(_) => ListDirectoryArgs {
                path: ".".to_string(),
            },
        };

        // 规范化路径
        let path = if PathBuf::from(&params.path).is_absolute() {
            PathBuf::from(&params.path)
        } else {
            workdir.join(&params.path)
        };

        debug!("列出目录: {:?}", path);

        // 检查目录是否存在
        if !path.exists() {
            return Ok(format!("Error: directory not found: {}", params.path));
        }

        if !path.is_dir() {
            return Ok(format!("Error: {} is not a directory", params.path));
        }

        // 读取目录内容
        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory: {:?}", e);
                return Ok(format_io_error(&e));
            }
        };

        let mut files = Vec::new();
        let mut dirs = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                dirs.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }

        // 排序
        dirs.sort();
        files.sort();

        // 构建输出
        let mut result = format!("Directory: {}\n", params.path);
        result.push_str(&format!("({} dirs, {} files)\n", dirs.len(), files.len()));
        result.push_str("---\n");

        for dir in &dirs {
            result.push_str(&format!("[DIR]  {}\n", dir));
        }
        for file in &files {
            result.push_str(&format!("[FILE] {}\n", file));
        }

        Ok(result)
    }
}
