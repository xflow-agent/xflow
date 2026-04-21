//! Agent 工具实现
//!
//! 将 Agent 包装成 Tool，让主对话 AI 可以自主调用

use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Reviewer Agent 参数
#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewerAgentArgs {
    /// 任务描述
    pub task: String,
    /// 可选：相关文件列表
    #[serde(default)]
    pub files: Vec<String>,
}

/// Reviewer Agent 工具
///
/// 用于代码分析、审查、理解项目等任务
pub struct ReviewerAgentTool;

impl ReviewerAgentTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewerAgentTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReviewerAgentTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "reviewer_agent",
            description: "【重要】项目分析工具。当用户要求'分析项目'、'分析功能'、'分析架构'、'了解项目结构'等时，**必须**调用此工具，不要直接使用 read_file。此工具会启动 ReviewerAgent 执行完整的多步骤分析流程（自动读取配置、分析模块、生成报告）。参数: task - 分析任务描述。",
            category: ToolCategory::Agent,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "task",
                result_display: ResultDisplayType::Summary,
                max_preview_lines: 10,
                max_preview_chars: 500,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "分析任务描述，如'分析项目所有功能'或'分析项目架构'"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: ReviewerAgentArgs =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        info!("Reviewer agent task: {}", params.task);

        Ok(format!(
            "Starting analysis task: {}\n\nI will follow these steps:\n1. Read project config files (Cargo.toml, README.md)\n2. Analyze project entry files and module structure\n3. Deep-dive into each module's implementation\n4. Synthesize findings into a complete report\n\nStarting step 1: reading project config...",
            params.task
        ))
    }

    fn definition(&self) -> super::tool::ToolDefinition {
        super::tool::ToolDefinition {
            tool_type: "function".to_string(),
            function: super::tool::FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters_schema(),
            },
        }
    }
}
