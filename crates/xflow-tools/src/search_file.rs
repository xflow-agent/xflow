//! search_file 工具实现 (使用 ripgrep)

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Command;
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
            description: "在文件中搜索内容（使用 ripgrep）。参数: pattern - 搜索模式（支持正则），path - 搜索路径（可选，默认当前目录），ignore_case - 是否忽略大小写",
            category: ToolCategory::Search,
            requires_confirmation: false,
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

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: SearchFileArgs = serde_json::from_value(args)?;

        debug!(
            "搜索文件: pattern={}, path={:?}",
            params.pattern, params.path
        );

        // 构建 rg 命令
        let mut cmd = Command::new("rg");

        cmd.arg(&params.pattern);

        if let Some(path) = &params.path {
            cmd.arg(path);
        }

        if params.ignore_case {
            cmd.arg("-i");
        }

        // 添加输出格式选项
        cmd.arg("--line-number") // 显示行号
            .arg("--color=never") // 禁用颜色
            .arg("-n"); // 简洁格式

        // 执行命令
        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Ok("错误: 未找到 ripgrep (rg) 命令，请确保已安装".to_string());
                }
                return Ok(format!("错误: 执行搜索失败: {}", e));
            }
        };

        // 处理结果
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();

            if lines.is_empty() {
                return Ok("未找到匹配项".to_string());
            }

            let mut result = format!("找到 {} 处匹配:\n", lines.len());
            result.push_str("---\n");

            // 限制输出行数
            let max_lines = 50;
            for line in lines.iter().take(max_lines) {
                result.push_str(&format!("{}\n", line));
            }

            if lines.len() > max_lines {
                result.push_str(&format!(
                    "... 还有 {} 行结果未显示\n",
                    lines.len() - max_lines
                ));
            }

            Ok(result)
        } else {
            // rg 返回非 0 可能是没有匹配项
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.is_empty() {
                Ok("未找到匹配项".to_string())
            } else {
                Ok(format!("错误: {}", stderr))
            }
        }
    }
}
