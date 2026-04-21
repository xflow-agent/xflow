use super::agent_executor::AgentExecutor;
use super::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewerAgentArgs {
    pub task: String,
    #[serde(default)]
    pub files: Vec<String>,
}

pub struct ReviewerAgentTool {
    executor: Option<Arc<dyn AgentExecutor>>,
}

impl ReviewerAgentTool {
    pub fn new() -> Self {
        Self { executor: None }
    }

    pub fn with_executor(executor: Arc<dyn AgentExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
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
            description: "Project analysis tool. Use this when the user asks to 'analyze project', 'analyze features', 'analyze architecture', 'understand project structure', etc. This tool launches a ReviewerAgent that performs a complete multi-step analysis (reads config, analyzes modules, generates report). Parameter: task - analysis task description.",
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
                    "description": "Analysis task description, e.g. 'analyze all project features' or 'analyze project architecture'"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _workdir: &std::path::Path,
    ) -> anyhow::Result<String> {
        let params: ReviewerAgentArgs =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("Failed to parse args: {}", e))?;

        info!("Reviewer agent task: {}", params.task);

        if let Some(ref executor) = self.executor {
            Ok(executor.execute_agent("reviewer_agent", serde_json::to_value(&params)?).await)
        } else {
            Ok(format!(
                "Starting analysis task: {}\n\nNo agent executor configured. Analysis will be performed by the main conversation loop.",
                params.task
            ))
        }
    }
}
