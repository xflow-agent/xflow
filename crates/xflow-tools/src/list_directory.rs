//! list_directory 工具实现

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

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
            description: "列出目录内容。参数: path - 目录路径，默认为当前目录",
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
                    "description": "要列出的目录路径，默认为当前目录"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: ListDirectoryArgs = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(_) => ListDirectoryArgs {
                path: ".".to_string(),
            },
        };

        let path = PathBuf::from(&params.path);

        debug!("列出目录: {:?}", path);

        // 检查目录是否存在
        if !path.exists() {
            return Ok(format!("错误: 目录不存在: {}", params.path));
        }

        // 检查是否为目录
        if !path.is_dir() {
            return Ok(format!("错误: {} 不是目录", params.path));
        }

        // 读取目录内容
        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(e) => e,
            Err(e) => {
                warn!("读取目录失败: {:?}", e);
                return Ok(format!("错误: 无法读取目录 {}: {}", params.path, e));
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
        let mut result = format!("目录: {}\n", params.path);
        result.push_str(&format!("({} 目录, {} 文件)\n", dirs.len(), files.len()));
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
