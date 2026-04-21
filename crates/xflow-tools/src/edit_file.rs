//! edit_file 工具实现
//!
//! 基于 diff 的精确文件编辑，避免大文件重写

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};
use xflow_model::format_io_error;

/// edit_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct EditFileArgs {
    /// 文件路径
    pub path: String,
    /// 要替换的旧内容（必须精确匹配）
    pub old_string: String,
    /// 新内容
    pub new_string: String,
}

/// edit_file 工具
///
/// 用于精确编辑文件中的特定内容，基于字符串替换
pub struct EditFileTool;

impl EditFileTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for EditFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "edit_file",
            description: "编辑文件内容。参数: path - 文件路径, old_string - 要替换的旧内容（必须精确匹配）, new_string - 新内容。用于精确修改文件的特定部分。",
            category: ToolCategory::File,
            danger_level: 1,
            display: ToolDisplayConfig {
                primary_param: "path",
                result_display: ResultDisplayType::Summary,
                max_preview_lines: 5,
                max_preview_chars: 300,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要编辑的文件路径"
                },
                "old_string": {
                    "type": "string",
                    "description": "要替换的旧内容（必须精确匹配，包括空格和换行）"
                },
                "new_string": {
                    "type": "string",
                    "description": "新内容"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: EditFileArgs = serde_json::from_value(args)?;

        debug!("编辑文件: {}", params.path);

        // 安全检查：目录遍历攻击
        if params.path.contains("..") || params.path.contains("~") {
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

        // 检查文件是否存在
        if !path.exists() {
            return Ok(format!("Error: file not found: {}", params.path));
        }

        if !path.is_file() {
            return Ok(format!("Error: {} is not a file", params.path));
        }

        // 读取文件内容
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read file: {:?}", e);
                return Ok(format_io_error(&e));
            }
        };

        // 检查旧内容是否存在
        if !content.contains(&params.old_string) {
            return Ok("Error: old_string not found in file.\n\
                Tip: ensure old_string exactly matches the file content (including whitespace and newlines).\n\
                Suggestion: use read_file first to view the file content."
                .to_string());
        }

        let match_count = content.matches(&params.old_string).count();
        if match_count > 1 {
            return Ok(format!(
                "Error: found {} matches for old_string.\n\
                Tip: old_string must uniquely identify the replacement location.\n\
                Suggestion: include more context in old_string to ensure uniqueness.",
                match_count
            ));
        }

        // 执行替换
        let new_content = content.replace(&params.old_string, &params.new_string);

        // 原子写入：先写入临时文件，再重命名
        let temp_path = path.with_extension("tmp");
        match tokio::fs::write(&temp_path, &new_content).await {
            Ok(_) => {
                match tokio::fs::rename(&temp_path, &path).await {
                    Ok(_) => {
                        info!("File edited successfully: {:?}", path);

                        let old_lines = params.old_string.lines().count();
                        let new_lines = params.new_string.lines().count();
                        let line_diff = new_lines as i32 - old_lines as i32;

                        let result = if line_diff > 0 {
                            format!(
                                "Edited: {} (removed {} lines, added {} lines, +{} net)",
                                params.path, old_lines, new_lines, line_diff
                            )
                        } else if line_diff < 0 {
                            format!(
                                "Edited: {} (removed {} lines, added {} lines, {} net)",
                                params.path, old_lines, new_lines, line_diff
                            )
                        } else {
                            format!(
                                "Edited: {} (replaced {} lines)",
                                params.path, old_lines
                            )
                        };

                        Ok(result)
                    }
                    Err(e) => {
                        warn!("Failed to rename file: {:?}", e);
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        Ok(format_io_error(&e))
                    }
                }
            }
            Err(e) => {
                warn!("Failed to write temp file: {:?}", e);
                Ok(format_io_error(&e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_edit_file_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Hello World").unwrap();
        writeln!(temp_file, "Line 2").unwrap();

        let tool = EditFileTool::new();
        let args = serde_json::json!({
            "path": temp_file.path().to_str().unwrap(),
            "old_string": "Hello World",
            "new_string": "Hello Rust"
        });

        let result = tool.execute(args, &std::env::current_dir().unwrap()).await.unwrap();
        assert!(result.contains("Edited:"));

        let content: String = tokio::fs::read_to_string(temp_file.path()).await.unwrap();
        assert!(content.contains("Hello Rust"));
        assert!(!content.contains("Hello World"));
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let tool = EditFileTool::new();
        let args = serde_json::json!({
            "path": "/nonexistent/file.txt",
            "old_string": "old",
            "new_string": "new"
        });

        let result = tool.execute(args, &std::env::current_dir().unwrap()).await.unwrap();
        assert!(result.contains("file not found"));
    }

    #[tokio::test]
    async fn test_edit_file_old_string_not_found() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Hello World").unwrap();

        let tool = EditFileTool::new();
        let args = serde_json::json!({
            "path": temp_file.path().to_str().unwrap(),
            "old_string": "Nonexistent",
            "new_string": "New"
        });

        let result = tool.execute(args, &std::env::current_dir().unwrap()).await.unwrap();
        assert!(result.contains("old_string not found"));
    }

    #[tokio::test]
    async fn test_edit_file_multiple_matches() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Hello").unwrap();
        writeln!(temp_file, "Hello").unwrap();

        let tool = EditFileTool::new();
        let args = serde_json::json!({
            "path": temp_file.path().to_str().unwrap(),
            "old_string": "Hello",
            "new_string": "Hi"
        });

        let result = tool.execute(args, &std::env::current_dir().unwrap()).await.unwrap();
        assert!(result.contains("found 2 matches"));
    }
}
