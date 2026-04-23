use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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

type TokenUsageCallback = Arc<Mutex<Option<Box<dyn FnMut(u32, u32, u32) + Send>>>>;

pub struct ToolLoop {
    provider: Arc<dyn ModelProvider>,
    tools: ToolRegistry,
    ui: Arc<dyn UiAdapter>,
    workdir: PathBuf,
    config: XflowConfig,
    /// Token 使用统计更新回调
    token_usage_callback: TokenUsageCallback,
    /// 会话总 Token 使用量
    session_token_usage: u32,
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
            token_usage_callback: Arc::new(Mutex::new(None)),
            session_token_usage: 0,
        }
    }

    /// 设置 Token 使用统计更新回调
    pub fn with_token_usage_callback<F>(self, callback: F) -> Self
    where
        F: FnMut(u32, u32, u32) + Send + 'static,
    {
        *self.token_usage_callback.lock().unwrap() = Some(Box::new(callback));
        self
    }

    /// 获取会话总 Token 使用量
    fn get_session_token_usage(&self) -> u32 {
        self.session_token_usage
    }

    /// 重置会话 Token 使用量
    #[allow(dead_code)]
    pub fn reset_session_token_usage(&mut self) {
        self.session_token_usage = 0;
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.definitions()
    }

    pub async fn run(&mut self, messages: &mut Vec<Message>) -> Result<ToolLoopResult> {
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
        &mut self,
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
                    usage,
                }) => {
                    // 处理 usage 信息
                    if let Some(usage_info) = &usage {
                        debug!("Received usage info: prompt_tokens={}, completion_tokens={}, total_tokens={}", 
                               usage_info.prompt_tokens, usage_info.completion_tokens, usage_info.total_tokens);
                        // 更新会话总 Token 使用量
                        self.session_token_usage += usage_info.total_tokens;
                        // 调用 token_usage_callback 回调
                        if let Ok(mut callback_opt) = self.token_usage_callback.lock() {
                            if let Some(callback) = &mut *callback_opt {
                                callback(usage_info.prompt_tokens, usage_info.completion_tokens, usage_info.total_tokens);
                            }
                        }
                        // 发送 TokenUsage 事件到 UI
                        let session_tokens = self.get_session_token_usage();
                        self.ui.output(OutputEvent::TokenUsage {
                            prompt: usage_info.prompt_tokens,
                            completion: usage_info.completion_tokens,
                            total: usage_info.total_tokens,
                            session: session_tokens,
                        }).await;
                    }

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

                    // 处理思考内容
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

                    // 处理正文内容
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
                    let preview: String = result
                        .lines()
                        .take(t.metadata().display.max_preview_lines)
                        .collect::<Vec<_>>()
                        .join("\n");
                    ToolResultDisplay::LineCount {
                        lines,
                        preview,
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
        use xflow_context::TokenEstimator;
        
        let system_msg_count = if !messages.is_empty() && messages[0].role == xflow_model::Role::System
        {
            1
        } else {
            0
        };

        // 获取模型的最大上下文长度（默认 4096 tokens）
        let max_context_tokens = self.config.session().max_context_tokens;
        
        // 计算当前消息的 token 数量
        let estimator = TokenEstimator::new();
        let mut total_tokens = 0;
        
        for msg in &mut *messages {
            if let Some(content) = &msg.content {
                total_tokens += estimator.estimate(content);
            }
        }
        
        // 如果 token 数量超过最大上下文长度，进行智能修剪
        if total_tokens > max_context_tokens {
            // 保留系统提示
            let mut new_messages = Vec::new();
            if system_msg_count > 0 {
                new_messages.push(messages[0].clone());
                if let Some(content) = &messages[0].content {
                    total_tokens -= estimator.estimate(content);
                }
            }
            
            // 从后往前添加消息，直到 token 数量接近最大上下文长度
            let mut temp_tokens = total_tokens;
            let mut temp_messages = Vec::new();
            
            for msg in messages.iter().rev() {
                if msg.role == xflow_model::Role::System {
                    continue; // 跳过系统提示，已经添加了
                }
                
                if let Some(content) = &msg.content {
                    let msg_tokens = estimator.estimate(content);
                    if temp_tokens - msg_tokens > (max_context_tokens as f64 * 0.8) as usize { // 保留 80% 的空间
                        temp_tokens -= msg_tokens;
                    } else {
                        temp_messages.push(msg.clone());
                    }
                } else {
                    // 如果消息没有内容，直接添加
                    temp_messages.push(msg.clone());
                }
            }
            
            // 反转消息顺序，使其恢复正常顺序
            temp_messages.reverse();
            new_messages.extend(temp_messages);
            
            // 添加省略提示
            new_messages.push(Message::system(
                "[some history messages omitted to save context space]",
            ));

            *messages = new_messages;
            warn!(
                "Message history trimmed based on token count, keeping ~{} tokens",
                (max_context_tokens as f64 * 0.8) as usize
            );
        }
        
        // 同时保留基于消息数量的修剪，作为后备
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
