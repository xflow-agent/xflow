use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, warn};
use xflow_model::{Message, ModelProvider, StreamChunk, ToolCall, ToolDefinition};
use xflow_tools::ToolRegistry;

use crate::config::XflowConfig;
use crate::events::*;
use crate::thinking_animation::ThinkingAnimation;
use crate::ui_adapter::UiAdapter;

fn format_tool_params(tool: &dyn xflow_tools::Tool, args: &serde_json::Value) -> String {
    tool.format_params(args)
}

fn convert_confirmation_request(
    tool_name: &str,
    req: xflow_tools::ToolConfirmationRequest,
) -> ConfirmationRequest {
    let mut event_req = ConfirmationRequest::new(tool_name, req.message);
    if req.danger_level > 0 {
        event_req = event_req.with_danger(
            req.danger_level,
            req.danger_reason
                .unwrap_or_else(|| format!("danger level: {}", req.danger_level)),
        );
    }
    event_req
}

pub struct ToolLoopResult {
    pub tools_called: usize,
    pub loops: usize,
}

pub struct ToolLoop {
    provider: Arc<dyn ModelProvider>,
    tools: ToolRegistry,
    ui: Arc<dyn UiAdapter>,
    workdir: PathBuf,
    config: XflowConfig,
}

impl ToolLoop {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tools: ToolRegistry,
        ui: Arc<dyn UiAdapter>,
        workdir: PathBuf,
        config: XflowConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            ui,
            workdir,
            config,
        }
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.definitions()
    }

    pub async fn run(&self, messages: &mut Vec<Message>) -> Result<ToolLoopResult> {
        let mut loop_count = 0;
        let mut total_tools_called = 0;

        loop {
            if self.ui.is_interrupted() {
                self.handle_interrupt().await;
                return Ok(ToolLoopResult {
                    tools_called: total_tools_called,
                    loops: loop_count,
                });
            }

            if loop_count >= self.config.tools().max_tool_loops {
                warn!(
                    "Max tool loop count reached: {}",
                    self.config.tools().max_tool_loops
                );
                self.ui
                    .output(OutputEvent::Error {
                        message: format!(
                            "Max loop count reached ({}), stopping",
                            self.config.tools().max_tool_loops
                        ),
                    })
                    .await;
                break;
            }

            if loop_count > 0 {
                self.ui
                    .output(OutputEvent::LoopProgress {
                        current: loop_count + 1,
                        max: self.config.tools().max_tool_loops,
                    })
                    .await;
            }

            if self.config.ui().show_thinking {
                self.ui.output(OutputEvent::ThinkingStart).await;
            }

            let mut animation = ThinkingAnimation::start(
                self.ui.clone(),
                self.config.ui().show_thinking,
                self.config.session().dot_animation_max,
            );

            let tool_defs = self.tool_definitions();
            let stream = self.provider.chat_stream(messages.clone(), tool_defs).await;

            let stream_result = self
                .process_stream(stream, &mut animation)
                .await;

            match stream_result {
                StreamOutcome::Content(response) => {
                    if !response.is_empty() {
                        messages.push(Message::assistant(&response));
                    }
                    self.trim_message_history(messages);

                    if total_tools_called > 0 {
                        self.ui
                            .output(OutputEvent::Done {
                                tools_called: total_tools_called,
                                loops: loop_count,
                            })
                            .await;
                    }

                    animation.stop();
                    animation.finish().await;
                    break;
                }
                StreamOutcome::ToolCalls(tool_calls) => {
                    debug!("Received {} tool calls", tool_calls.len());
                    messages.push(Message::assistant_with_tools(tool_calls.clone()));

                    for (i, tool_call) in tool_calls.iter().enumerate() {
                        if self.ui.is_interrupted() {
                            animation.stop();
                            animation.finish().await;
                            self.handle_interrupt().await;
                            return Ok(ToolLoopResult {
                                tools_called: total_tools_called,
                                loops: loop_count,
                            });
                        }

                        self.execute_tool_call(tool_call, i, tool_calls.len(), messages)
                            .await;
                        total_tools_called += 1;
                    }

                    animation.stop();
                    animation.finish().await;
                    loop_count += 1;
                }
                StreamOutcome::Interrupted => {
                    animation.stop();
                    animation.finish().await;
                    self.handle_interrupt().await;
                    return Ok(ToolLoopResult {
                        tools_called: total_tools_called,
                        loops: loop_count,
                    });
                }
                StreamOutcome::Retry => {
                    animation.stop();
                    animation.finish().await;
                    loop_count += 1;
                    if loop_count >= self.config.tools().max_tool_loops {
                        self.ui
                            .output(OutputEvent::Error {
                                message: "Max retry count reached, please check network or server config"
                                    .to_string(),
                            })
                            .await;
                        return Err(anyhow::anyhow!("Max retry count reached"));
                    }
                    continue;
                }
                StreamOutcome::Cancelled => {
                    animation.stop();
                    animation.finish().await;
                    break;
                }
            }
        }

        Ok(ToolLoopResult {
            tools_called: total_tools_called,
            loops: loop_count,
        })
    }

    async fn process_stream(
        &self,
        stream: std::pin::Pin<Box<dyn futures::Stream<Item = xflow_model::Result<StreamChunk>> + Send>>,
        animation: &mut ThinkingAnimation,
    ) -> StreamOutcome {
        use futures::StreamExt;

        let mut full_response = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_tool_call_mode = false;
        let mut animation_stopped = false;

        let mut stream = stream;

        while let Some(chunk) = stream.next().await {
            if self.ui.is_interrupted() {
                return StreamOutcome::Interrupted;
            }

            match chunk {
                Ok(StreamChunk {
                    content,
                    reasoning,
                    done,
                    tool_calls: chunk_tool_calls,
                }) => {
                    if !chunk_tool_calls.is_empty() {
                        if !animation_stopped {
                            animation.stop();
                            animation_stopped = true;
                        }
                        if !in_tool_call_mode {
                            in_tool_call_mode = true;
                        }
                        tool_calls.extend(chunk_tool_calls);
                    }

                    if let Some(reasoning_text) = reasoning {
                        if !reasoning_text.is_empty() {
                            if !animation_stopped {
                                animation.stop();
                                animation_stopped = true;
                            }
                            if self.config.ui().show_thinking {
                                self.ui
                                    .output(OutputEvent::ThinkingContent {
                                        text: reasoning_text.clone(),
                                    })
                                    .await;
                            }
                        }
                    }

                    if !in_tool_call_mode && !content.is_empty() && !content.trim().is_empty() {
                        if !animation_stopped {
                            animation.stop();
                            animation_stopped = true;
                        }
                        full_response.push_str(&content);
                        self.ui
                            .output(OutputEvent::Content { text: content })
                            .await;
                    }

                    if done {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Stream response error: {}", e);

                    animation.stop();

                    let error_msg = if e.to_string().contains("timeout") {
                        "Request timed out".to_string()
                    } else if e.to_string().contains("connection") {
                        "Connection error".to_string()
                    } else if self.config.ui().verbose_errors {
                        format!("Request failed: {}", e)
                    } else {
                        "Request failed".to_string()
                    };

                    self.ui
                        .output(OutputEvent::Error {
                            message: error_msg.clone(),
                        })
                        .await;

                    let retry_req = ConfirmationRequest::new("retry", &error_msg);
                    if self.ui.confirm(retry_req.with_danger(0, "")).await {
                        self.ui
                            .output(OutputEvent::Content {
                                text: "\nRetrying...\n".to_string(),
                            })
                            .await;
                        return StreamOutcome::Retry;
                    } else {
                        self.ui
                            .output(OutputEvent::Content {
                                text: "\nOperation cancelled. Enter a new command to continue."
                                    .to_string(),
                            })
                            .await;
                        return StreamOutcome::Cancelled;
                    }
                }
            }
        }

        if !tool_calls.is_empty() {
            StreamOutcome::ToolCalls(tool_calls)
        } else {
            StreamOutcome::Content(full_response)
        }
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
        index: usize,
        total: usize,
        messages: &mut Vec<Message>,
    ) {
        let tool_name = &tool_call.function.name;
        let tool_args = &tool_call.function.arguments;

        let tool = self.tools.get(tool_name);

        let params_display = if total > 1 {
            format!("[{}/{}]", index + 1, total)
        } else if let Some(ref t) = tool {
            format_tool_params(t.as_ref(), tool_args)
        } else {
            String::new()
        };

        self.ui
            .output(OutputEvent::ToolCall {
                name: tool_name.clone(),
                params_display,
                args: tool_args.clone(),
            })
            .await;

        debug!("Tool args: {}", tool_args);

        let confirmed = if let Some(ref t) = tool {
            if let Some(tool_req) = t.build_confirmation(tool_args) {
                let req = convert_confirmation_request(tool_name, tool_req);
                self.ui.confirm(req).await
            } else {
                true
            }
        } else {
            true
        };

        let (result, success) = if !confirmed {
            ("Operation cancelled".to_string(), false)
        } else if let Some(ref tool) = tool {
            match tool.execute(tool_args.clone(), &self.workdir).await {
                Ok(result) => (result, true),
                Err(e) => (format!("Tool execution error: {}", e), false),
            }
        } else {
            (format!("Unknown tool: {}", tool_name), false)
        };

        let display_text = if let Some(ref t) = tool {
            let (text, _) = t.format_result(&result);
            match t.metadata().display.result_display {
                xflow_tools::ResultDisplayType::Full => ToolResultDisplay::Full { content: text },
                xflow_tools::ResultDisplayType::LineCount => {
                    let lines = result.lines().count();
                    ToolResultDisplay::LineCount {
                        lines,
                        preview: text,
                    }
                }
                xflow_tools::ResultDisplayType::ByteSize => ToolResultDisplay::ByteSize { size: text },
                xflow_tools::ResultDisplayType::StatusOnly => ToolResultDisplay::StatusOnly,
                xflow_tools::ResultDisplayType::Summary => ToolResultDisplay::Summary { text },
            }
        } else {
            ToolResultDisplay::Summary {
                text: result.clone(),
            }
        };

        self.ui
            .output(OutputEvent::ToolResult {
                name: tool_name.clone(),
                result: ToolResultData {
                    full_result: result.clone(),
                    display: display_text,
                    size: result.len(),
                    success,
                },
            })
            .await;

        debug!("Tool result: {} bytes", result.len());

        let truncated_result = if result.len() > self.config.tools().max_tool_result_size {
            format!(
                "{}\n\n[result truncated, original size: {} chars]",
                &result[..self.config.tools().max_tool_result_size],
                result.len()
            )
        } else {
            result
        };

        messages.push(Message::tool_result(tool_name, truncated_result));
    }

    async fn handle_interrupt(&self) {
        let info = self.ui.get_interrupt_info();
        let reason = info
            .as_ref()
            .map(|i| i.reason.as_str())
            .unwrap_or("user interrupted");

        warn!("Execution interrupted: {}", reason);

        self.ui
            .output(OutputEvent::Error {
                message: format!("Execution interrupted: {}", reason),
            })
            .await;

        self.ui.clear_interrupt();
    }

    fn trim_message_history(&self, messages: &mut Vec<Message>) {
        let system_msg_count = if !messages.is_empty() && messages[0].role == xflow_model::Role::System
        {
            1
        } else {
            0
        };

        let total_messages = messages.len();
        let max_non_system = self.config.session().max_message_history;

        if total_messages > system_msg_count + max_non_system {
            let keep_start = total_messages - max_non_system;
            let mut new_messages = Vec::with_capacity(system_msg_count + max_non_system);

            if system_msg_count > 0 {
                new_messages.push(messages[0].clone());
            }

            new_messages.extend_from_slice(&messages[keep_start..]);
            new_messages.push(Message::system(
                "[some history messages omitted to save context space]",
            ));

            *messages = new_messages;
            warn!(
                "Message history trimmed, keeping last {} messages",
                max_non_system
            );
        }
    }
}

enum StreamOutcome {
    Content(String),
    ToolCalls(Vec<ToolCall>),
    Interrupted,
    Retry,
    Cancelled,
}
