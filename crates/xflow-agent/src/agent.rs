//! Agent core abstraction
//!
//! Defines common interfaces and shared types for all Agents

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xflow_model::ModelProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentType {
    Reviewer,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Reviewer => write!(f, "Reviewer"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl Task {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            status: TaskStatus::Pending,
            error: None,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.status != TaskStatus::Pending {
            anyhow::bail!(
                "Task {} has status {:?}, cannot start",
                self.id,
                self.status
            );
        }
        self.status = TaskStatus::InProgress;
        Ok(())
    }

    pub fn complete(&mut self) -> anyhow::Result<()> {
        if self.status != TaskStatus::InProgress {
            anyhow::bail!(
                "Task {} has status {:?}, cannot complete",
                self.id,
                self.status
            );
        }
        self.status = TaskStatus::Completed;
        Ok(())
    }

    pub fn fail(&mut self, error: impl Into<String>) -> anyhow::Result<()> {
        self.status = TaskStatus::Failed;
        self.error = Some(error.into());
        Ok(())
    }

    pub fn skip(&mut self, reason: impl Into<String>) {
        self.status = TaskStatus::Skipped;
        self.error = Some(reason.into());
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped
        )
    }
}

#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub output: String,
    pub success: bool,
    pub tool_calls: Vec<ToolCallRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub name: String,
    pub result: String,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub workdir: std::path::PathBuf,
    pub language: Option<String>,
    pub relevant_files: Vec<String>,
    pub current_task: Option<Task>,
    pub tool_results: Vec<ToolResult>,
}

impl AgentContext {
    pub fn new(workdir: std::path::PathBuf) -> Self {
        Self {
            workdir,
            language: None,
            relevant_files: Vec::new(),
            current_task: None,
            tool_results: Vec::new(),
        }
    }

    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.tool_results.push(result);
    }

    pub fn tool_results_summary(&self) -> String {
        if self.tool_results.is_empty() {
            return String::new();
        }

        let mut summary = String::from("Executed tools and their results:\n");
        for (i, tr) in self.tool_results.iter().enumerate() {
            let truncated = if tr.result.len() > 3000 {
                let safe_end = tr
                    .result
                    .char_indices()
                    .take_while(|(idx, _)| *idx < 3000)
                    .last()
                    .map(|(idx, c)| idx + c.len_utf8())
                    .unwrap_or(0);
                format!(
                    "{}...(total {} bytes)",
                    &tr.result[..safe_end],
                    tr.result.len()
                )
            } else {
                tr.result.clone()
            };
            summary.push_str(&format!(
                "\n{}. Tool: {}\n   Status: {}\n   Result:\n{}\n",
                i + 1,
                tr.name,
                if tr.success { "success" } else { "failed" },
                truncated
            ));
        }
        summary
    }
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn agent_type(&self) -> AgentType;

    fn name(&self) -> &str;

    fn description(&self) -> &str;

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse>;

    fn system_prompt(&self) -> String;
}
