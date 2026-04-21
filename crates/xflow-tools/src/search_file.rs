//! search_file 工具实现 (内置搜索)

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

/// search_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchFileArgs {
    /// 搜索模式（正则表达式）
    pub pattern: String,
    /// 搜索路径（文件或目录）
    #[serde(default)]
    pub path: Option<String>,
    /// 是否忽略大小写
    #[serde(default)]
    pub ignore_case: bool,
}

/// search_file 工具
pub struct SearchFileTool;

impl SearchFileTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchFileTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "search_file",
            description: "在文件中搜索内容。参数: pattern - 搜索模式（支持正则），path - 搜索路径（可选，默认当前目录），ignore_case - 是否忽略大小写",
            category: ToolCategory::Search,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "pattern",
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
                "pattern": {
                    "type": "string",
                    "description": "搜索模式（支持正则表达式）"
                },
                "path": {
                    "type": "string",
                    "description": "搜索路径（文件或目录，默认当前目录）"
                },
                "ignore_case": {
                    "type": "boolean",
                    "description": "是否忽略大小写"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: SearchFileArgs = serde_json::from_value(args)?;

        debug!(
            "搜索文件: pattern={}, path={:?}",
            params.pattern, params.path
        );

        // 编译正则表达式
        let regex = if params.ignore_case {
            Regex::new(&format!("(?i){}", params.pattern))
        } else {
            Regex::new(&params.pattern)
        };

        let regex = match regex {
            Ok(r) => r,
            Err(e) => {
                return Ok(format!("Error: invalid regex: {}", e));
            }
        };

        // 确定搜索路径
        let search_path = if let Some(path) = &params.path {
            if PathBuf::from(path).is_absolute() {
                PathBuf::from(path)
            } else {
                workdir.join(path)
            }
        } else {
            workdir.to_path_buf()
        };

        // 检查路径是否存在
        if !search_path.exists() {
            return Ok(format!("Error: search path not found: {}", search_path.display()));
        }

        // 收集搜索结果
        let mut matches = Vec::new();
        
        // 使用 ignore crate 遍历文件
        let walker = WalkBuilder::new(&search_path)
            .standard_filters(true)
            .build();

        for entry in walker {
            match entry {
                Ok(entry) => {
                    let path = entry.path().to_path_buf();
                    if path.is_file() {
                        // 尝试读取文件内容
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            // 逐行搜索
                            for (line_num, line) in content.lines().enumerate() {
                                if regex.is_match(line) {
                                    let relative_path = path.strip_prefix(workdir)
                                        .unwrap_or(&path)
                                        .to_path_buf();
                                    matches.push((relative_path, line_num + 1, line.to_string()));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("搜索错误: {}", e);
                }
            }
        }

        // 处理结果
        if matches.is_empty() {
            return Ok("No matches found".to_string());
        }

        let mut result = format!("Found {} matches:\n", matches.len());
        result.push_str("---\n");

        let max_lines = 50;
        for (path, line_num, line) in matches.iter().take(max_lines) {
            result.push_str(&format!("{}:{}:{}\n", path.display(), line_num, line));
        }

        if matches.len() > max_lines {
            result.push_str(&format!(
                "... {} more results not shown\n",
                matches.len() - max_lines
            ));
        }

        Ok(result)
    }
}
