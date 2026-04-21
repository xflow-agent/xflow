use crate::agent::{Agent, AgentContext, AgentResponse, AgentType, Task, ToolCallRequest};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;
use xflow_model::{get_reviewer_prompt, Message, ModelProvider, ToolDefinition};

pub struct ReviewerAgent {
    tool_definitions: Vec<ToolDefinition>,
}

impl ReviewerAgent {
    pub fn new(tool_definitions: Vec<ToolDefinition>) -> Self {
        Self { tool_definitions }
    }
}

impl Default for ReviewerAgent {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[async_trait]
impl Agent for ReviewerAgent {
    fn agent_type(&self) -> AgentType {
        AgentType::Reviewer
    }

    fn name(&self) -> &str {
        "reviewer"
    }

    fn description(&self) -> &str {
        "Code reviewer for analysis, issue detection, and quality assessment"
    }

    fn system_prompt(&self) -> String {
        get_reviewer_prompt()
    }

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse> {
        info!("Reviewer starting review: {}", task.description);

        let system_prompt = self.system_prompt();
        let mut messages = vec![Message::system(&system_prompt)];

        let mut context_parts = Vec::new();

        if !context.relevant_files.is_empty() {
            context_parts.push(format!(
                "Files to review:\n{}",
                context.relevant_files.join("\n")
            ));
        }

        let has_tool_results = !context.tool_results.is_empty();

        let context_info = if context_parts.is_empty() {
            String::new()
        } else {
            context_parts.join("\n\n") + "\n\n"
        };

        let user_prompt = if has_tool_results {
            let tool_results = context.tool_results_summary();
            format!(
                "{}{}\n\nYou have executed tool calls and obtained results.\n\
                Based on the tool execution results above, continue the review task: {}\n\n\
                You can:\n\
                1. Output the analysis report directly based on existing results\n\
                2. Call more tools if additional information is needed\n\
                \n\
                Please continue:",
                context_info, tool_results, task.description
            )
        } else {
            format!(
                "{}Review task:\n{}\n\n\
                You can use available tools to gather information, then output analysis results.",
                context_info, task.description
            )
        };

        messages.push(Message::user(&user_prompt));

        let stream = provider
            .chat_stream(messages.clone(), self.tool_definitions.clone())
            .await;

        use futures::StreamExt;
        let mut stream = stream;
        let mut full_response = String::new();
        let mut tool_calls = Vec::new();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    if !chunk.content.is_empty() {
                        full_response.push_str(&chunk.content);
                    }

                    for tc in chunk.tool_calls {
                        tool_calls.push(ToolCallRequest {
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        });
                    }

                    if chunk.done {
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Model call failed: {}", e));
                }
            }
        }

        Ok(AgentResponse {
            output: full_response,
            success: true,
            tool_calls,
        })
    }
}
