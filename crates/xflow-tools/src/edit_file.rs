//! edit_file 工具实现
//!
//! 基于 diff 的精确文件编辑，避免大文件重写

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

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
            requires_confirmation: true,
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

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: EditFileArgs = serde_json::from_value(args)?;
        let path = PathBuf::from(&params.path);

        debug!("编辑文件: {:?}", path);

        // 安全检查：目录遍历攻击
        if params.path.contains("..") || params.path.contains("~") {
            warn!("检测到目录遍历攻击尝试: {}", params.path);
            return Ok(format!("错误: 路径包含非法字符: {}", params.path));
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
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                warn!("读取文件失败: {:?}", e);
                return Ok(format!("错误: 无法读取文件 {}: {}", params.path, e));
            }
        };

        // 检查旧内容是否存在
        if !content.contains(&params.old_string) {
            return Ok("错误: 在文件中找不到指定的旧内容。\n\
                提示: 请确保 old_string 与文件中的内容完全匹配（包括空格和换行）。\n\
                建议先使用 read_file 工具查看文件内容。"
                .to_string());
        }

        // 统计匹配次数
        let match_count = content.matches(&params.old_string).count();
        if match_count > 1 {
            return Ok(format!(
                "错误: 找到 {} 处匹配的旧内容。\n\
                提示: old_string 必须唯一确定要替换的位置。\n\
                请扩大 old_string 的范围（包含更多上下文）以确保唯一性。",
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
                        info!("成功编辑文件: {:?}", path);

                        // 计算变更统计
                        let old_lines = params.old_string.lines().count();
                        let new_lines = params.new_string.lines().count();
                        let line_diff = new_lines as i32 - old_lines as i32;

                        let result = if line_diff > 0 {
                            format!(
                                "成功编辑文件: {}\n\
                                变更: 删除了 {} 行，添加了 {} 行（净增 {} 行）",
                                params.path, old_lines, new_lines, line_diff
                            )
                        } else if line_diff < 0 {
                            format!(
                                "成功编辑文件: {}\n\
                                变更: 删除了 {} 行，添加了 {} 行（净减 {} 行）",
                                params.path, old_lines, new_lines, -line_diff
                            )
                        } else {
                            format!(
                                "成功编辑文件: {}\n\
                                变更: 替换了 {} 行内容",
                                params.path, old_lines
                            )
                        };

                        Ok(result)
                    }
                    Err(e) => {
                        warn!("重命名文件失败: {:?}", e);
                        // 清理临时文件
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        Ok(format!("错误: 无法保存文件 {}: {}", params.path, e))
                    }
                }
            }
            Err(e) => {
                warn!("写入临时文件失败: {:?}", e);
                Ok(format!("错误: 无法写入临时文件: {}", e))
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

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("成功编辑文件"));

        // 验证文件内容
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

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("文件不存在"));
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

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("找不到指定的旧内容"));
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

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("找到 2 处匹配"));
    }
}
