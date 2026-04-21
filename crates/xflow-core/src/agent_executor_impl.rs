use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_agent::{Agent, AgentContext, ReviewerAgent, Task};
use xflow_model::ModelProvider;
use xflow_tools::{AgentExecutor, ToolRegistry};

use crate::config::XflowConfig;
use crate::events::*;
use crate::ui_adapter::UiAdapter;

pub struct SessionAgentExecutor {
    provider: Arc<dyn ModelProvider>,
    ui: Arc<dyn UiAdapter>,
    tools: ToolRegistry,
    workdir: PathBuf,
    config: XflowConfig,
}

impl SessionAgentExecutor {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        ui: Arc<dyn UiAdapter>,
        tools: ToolRegistry,
        workdir: PathBuf,
        config: XflowConfig,
    ) -> Self {
        Self {
            provider,
            ui,
            tools,
            workdir,
            config,
        }
    }
}

#[async_trait::async_trait]
impl AgentExecutor for SessionAgentExecutor {
    async fn execute_agent(&self, tool_name: &str, args: serde_json::Value) -> String {
        match tool_name {
            "reviewer_agent" => self.execute_reviewer_agent(args).await,
            _ => format!("Unknown agent: {}", tool_name),
        }
    }
}

impl SessionAgentExecutor {
    async fn execute_reviewer_agent(&self, args: serde_json::Value) -> String {
        let task_desc = args
            .get("task")
            .and_then(|t| t.as_str())
            .unwrap_or("analyze project");

        info!("Executing Reviewer Agent: {}", task_desc);
        self.ui
            .output(OutputEvent::Content {
                text: "\nStarting project analysis...\n".to_string(),
            })
            .await;

        match tokio::time::timeout(
            std::time::Duration::from_secs(self.config.agent().execution_timeout),
            self.run_reviewer_agent_task(task_desc),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!("Agent execution timeout");
                self.ui
                    .output(OutputEvent::Error {
                        message: "Project analysis timed out, please try again later".to_string(),
                    })
                    .await;
                "Analysis timed out".to_string()
            }
        }
    }

    async fn run_reviewer_agent_task(&self, task_desc: &str) -> String {
        let tool_definitions = self.tools.definitions();
        let reviewer = ReviewerAgent::new(tool_definitions);
        let task_id = format!(
            "review-{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        let mut task = Task::new(&task_id, task_desc);

        if let Err(e) = task.start() {
            warn!("Task start failed: {}", e);
            return format!("Task start failed: {}", e);
        }

        debug!("Task {} started", task.id);

        let mut context = AgentContext::new(self.workdir.clone());
        let max_iterations = 10;
        let mut iteration = 0;

        loop {
            if self.ui.is_interrupted() {
                let _ = task.fail("user interrupted");
                return "Analysis interrupted".to_string();
            }

            if iteration >= max_iterations {
                warn!("Agent reached max iterations");
                let _ = task.fail("max iterations reached");
                break;
            }

            match reviewer
                .execute(&task, &context, self.provider.clone())
                .await
            {
                Ok(response) => {
                    if !response.output.is_empty() {
                        self.ui
                            .output(OutputEvent::Content {
                                text: response.output.clone(),
                            })
                            .await;
                    }

                    if response.tool_calls.is_empty() {
                        let _ = task.complete();
                        debug!("Task {} completed", task.id);
                        return response.output;
                    }

                    for tool_call in &response.tool_calls {
                        let tool_name = &tool_call.name;
                        let tool_args = &tool_call.arguments;

                        if tool_name == "reviewer_agent" {
                            continue;
                        }

                        self.ui
                            .output(OutputEvent::Content {
                                text: format!("\n[Agent calling tool: {}]", tool_name),
                            })
                            .await;

                        let result = if let Some(tool) = self.tools.get(tool_name) {
                            match tool.execute(tool_args.clone(), &self.workdir).await {
                                Ok(r) => r,
                                Err(e) => format!("Error: {}", e),
                            }
                        } else {
                            format!("Unknown tool: {}", tool_name)
                        };

                        context.add_tool_result(xflow_agent::ToolResult {
                            name: tool_name.clone(),
                            result,
                            success: true,
                        });
                    }

                    iteration += 1;
                }
                Err(e) => {
                    warn!("Agent execution failed: {}", e);
                    let _ = task.fail(e.to_string());
                    return format!("Analysis failed: {}", e);
                }
            }
        }

        "Analysis complete".to_string()
    }
}
