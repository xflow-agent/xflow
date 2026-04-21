//! 会话管理 - 使用 UiAdapter

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_agent::{Agent, AgentContext, ReviewerAgent, Task};
use xflow_context::ContextBuilder;
use xflow_model::{Message, ModelProvider, StreamChunk, ToolCall, ToolDefinition};
use xflow_tools::ToolRegistry;

use crate::config::XflowConfig;
use crate::events::*;
use crate::ui_adapter::{AutoConfirmAdapter, UiAdapter};
use xflow_model::get_system_prompt;

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

/// 会话状态 V2
pub struct Session {
    messages: Vec<Message>,
    provider: Arc<dyn ModelProvider>,
    workdir: PathBuf,
    model_name: String,
    tools: ToolRegistry,
    ui: Arc<dyn UiAdapter>,
    system_added: bool,
    project_context: Option<String>,
    config: XflowConfig,
}

impl Session {
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf, ui: Arc<dyn UiAdapter>) -> Self {
        let model_name = provider.model_info().name;
        let tools = xflow_tools::create_default_tools();
        Self {
            messages: Vec::new(),
            provider,
            workdir,
            model_name,
            tools,
            ui,
            system_added: false,
            project_context: None,
            config: XflowConfig::load(),
        }
    }

    /// 创建自动确认模式的会话（向后兼容）
    pub fn with_auto_confirm(
        provider: Arc<dyn ModelProvider>,
        workdir: PathBuf,
        auto: bool,
    ) -> Self {
        let ui = if auto {
            AutoConfirmAdapter::approving()
        } else {
            AutoConfirmAdapter::rejecting()
        };
        Self::new(provider, workdir, ui)
    }

    /// 初始化项目上下文
    pub fn init_project_context(&mut self) -> Result<()> {
        info!("初始化项目上下文：{:?}", self.workdir);

        let builder = ContextBuilder::new(self.workdir.clone());
        match builder.generate_system_context() {
            Ok(context) => {
                info!("项目上下文初始化成功");
                self.project_context = Some(context);
            }
            Err(e) => {
                warn!("项目上下文初始化失败：{}", e);
            }
        }

        Ok(())
    }

    /// 获取工具定义列表
    fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.definitions()
    }

    /// 处理用户输入
    pub async fn process(&mut self, input: &str) -> Result<()> {
        // 首次对话时添加系统提示词
        if !self.system_added {
            let system_prompt = if let Some(ref context) = self.project_context {
                format!("{}\n{}", get_system_prompt(), context)
            } else {
                get_system_prompt()
            };
            self.messages.push(Message::system(&system_prompt));
            self.system_added = true;
        }

        // 添加用户消息
        self.messages.push(Message::user(input));

        debug!("当前消息数量：{}", self.messages.len());

        // 工具调用循环
        let mut loop_count = 0;
        let mut total_tools_called = 0;

        loop {
            // 检查中断
            if self.ui.is_interrupted() {
                self.handle_interrupt().await;
                return Ok(());
            }

            if loop_count >= self.config.tools().max_tool_loops {
                warn!("Max tool loop count reached: {}", self.config.tools().max_tool_loops);
                self.ui
                    .output(OutputEvent::Error {
                        message: format!("Max loop count reached ({}), stopping", self.config.tools().max_tool_loops),
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

            // 发送思考中状态
            if self.config.ui().show_thinking {
                self.ui.output(OutputEvent::ThinkingStart).await;
            }

            let animation_running = Arc::new(AtomicBool::new(true));
            let show_thinking = self.config.ui().show_thinking;
            let animation_task = {
                let running = animation_running.clone();
                let ui = self.ui.clone();
                let dot_max = self.config.session().dot_animation_max;
                tokio::spawn(async move {
                    if !show_thinking {
                        return;
                    }
                    let mut interval =
                        tokio::time::interval(std::time::Duration::from_millis(1000));
                    let mut dot_count = 0;
                    loop {
                        interval.tick().await;
                        if !running.load(Ordering::Relaxed) {
                            break;
                        }
                        if dot_count < dot_max {
                            ui.output(OutputEvent::ThinkingDot).await;
                            dot_count += 1;
                        }
                    }
                })
            };

            // 调用模型
            let tool_defs = self.get_tool_definitions();
            let stream = self
                .provider
                .chat_stream(self.messages.clone(), tool_defs)
                .await;

            // 处理流式响应
            let mut full_response = String::new();
            let mut full_reasoning = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut in_tool_call_mode = false;
            let mut animation_stopped = false;

            use futures::StreamExt;
            let mut stream = stream;

            while let Some(chunk) = stream.next().await {
                // 检查中断（在处理每个 chunk 前检查）
                if self.ui.is_interrupted() {
                    animation_running.store(false, Ordering::Relaxed);
                    let _ = animation_task.await;
                    self.handle_interrupt().await;
                    return Ok(());
                }

                match chunk {
                    Ok(StreamChunk {
                        content,
                        reasoning,
                        done,
                        tool_calls: chunk_tool_calls,
                    }) => {
                        // 收集工具调用
                        if !chunk_tool_calls.is_empty() {
                            if !animation_stopped {
                                animation_running.store(false, Ordering::Relaxed);
                                animation_stopped = true;
                            }
                            if !in_tool_call_mode {
                                in_tool_call_mode = true;
                            }
                            tool_calls.extend(chunk_tool_calls);
                        }

                        // 输出思考内容
                        if let Some(reasoning_text) = reasoning {
                            if !reasoning_text.is_empty() {
                                if !animation_stopped {
                                    animation_running.store(false, Ordering::Relaxed);
                                    animation_stopped = true;
                                }
                                if self.config.ui().show_thinking {
                                    self.ui
                                        .output(OutputEvent::ThinkingContent {
                                            text: reasoning_text.clone(),
                                        })
                                        .await;
                                }
                                full_reasoning.push_str(&reasoning_text);
                            }
                        }

                        // 输出文本内容
                        if !in_tool_call_mode && !content.is_empty() && !content.trim().is_empty() {
                            if !animation_stopped {
                                animation_running.store(false, Ordering::Relaxed);
                                animation_stopped = true;
                            }
                            full_response.push_str(&content);
                            self.ui.output(OutputEvent::Content { text: content }).await;
                        }

                        if done {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("流式响应错误：{}", e);

                        if !animation_stopped {
                            animation_running.store(false, Ordering::Relaxed);
                            animation_stopped = true;
                        }

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
                        } else {
                            self.ui
                                .output(OutputEvent::Content {
                                    text: "\nOperation cancelled. Enter a new command to continue.".to_string(),
                                })
                                .await;
                            break;
                        }
                    }
                }
            }

            // 如果有工具调用，执行并继续循环
            if !tool_calls.is_empty() {
                debug!("收到 {} 个工具调用", tool_calls.len());

                // 添加助手消息
                self.messages
                    .push(Message::assistant_with_tools(tool_calls.clone()));

                // 执行每个工具调用
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    // 检查中断
                    if self.ui.is_interrupted() {
                        animation_running.store(false, Ordering::Relaxed);
                        let _ = animation_task.await;
                        self.handle_interrupt().await;
                        return Ok(());
                    }

                    let tool_name = &tool_call.function.name;
                    let tool_args = &tool_call.function.arguments;

                    // 获取工具（用于元数据）
                    let tool = self.tools.get(tool_name);

                    // 格式化参数显示
                    let params_display = if tool_calls.len() > 1 {
                        format!("[{}/{}]", i + 1, tool_calls.len())
                    } else if let Some(ref t) = tool {
                        format_tool_params(t.as_ref(), tool_args)
                    } else {
                        String::new()
                    };

                    // 发送工具调用事件
                    self.ui
                        .output(OutputEvent::ToolCall {
                            name: tool_name.clone(),
                            params_display,
                            args: tool_args.clone(),
                        })
                        .await;

                    debug!("工具参数：{}", tool_args);

                    // 检查是否需要确认（使用工具的 build_confirmation 方法）
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

                    // 执行工具
                    let (result, success) = if !confirmed {
                        ("Operation cancelled".to_string(), false)
                    } else if tool_name == "reviewer_agent" {
                        (self.execute_reviewer_agent(tool_args.clone()).await, true)
                    } else if let Some(ref tool) = tool {
                        match tool.execute(tool_args.clone(), &self.workdir).await {
                            Ok(result) => (result, true),
                            Err(e) => (format!("Tool execution error: {}", e), false),
                        }
                    } else {
                        (format!("Unknown tool: {}", tool_name), false)
                    };

                    // 构建工具结果展示（使用工具的 format_result 方法）
                    let display_text = if let Some(ref t) = tool {
                        let (text, _) = t.format_result(&result);
                        match t.metadata().display.result_display {
                            xflow_tools::ResultDisplayType::Full => {
                                ToolResultDisplay::Full { content: text }
                            }
                            xflow_tools::ResultDisplayType::LineCount => {
                                let lines = result.lines().count();
                                ToolResultDisplay::LineCount {
                                    lines,
                                    preview: text,
                                }
                            }
                            xflow_tools::ResultDisplayType::ByteSize => {
                                ToolResultDisplay::ByteSize { size: text }
                            }
                            xflow_tools::ResultDisplayType::StatusOnly => {
                                ToolResultDisplay::StatusOnly
                            }
                            xflow_tools::ResultDisplayType::Summary => {
                                ToolResultDisplay::Summary { text }
                            }
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

                    // 添加工具结果消息
                    self.messages
                        .push(Message::tool_result(tool_name, truncated_result));

                    // 检查消息历史是否超过限制
                    self.trim_message_history();

                    total_tools_called += 1;
                }

                // 确保动画任务停止
                animation_running.store(false, Ordering::Relaxed);
                let _ = animation_task.await;

                loop_count += 1;
                continue;
            }

            // 没有工具调用，添加助手消息并结束
            if !full_response.is_empty() {
                self.messages.push(Message::assistant(&full_response));
            }

            // 检查消息历史是否超过限制
            self.trim_message_history();

            // 显示执行统计
            if total_tools_called > 0 {
                self.ui
                    .output(OutputEvent::Done {
                        tools_called: total_tools_called,
                        loops: loop_count,
                    })
                    .await;
            }

            // 确保动画任务停止
            animation_running.store(false, Ordering::Relaxed);
            let _ = animation_task.await;

            break;
        }

        Ok(())
    }

    /// 清空会话
    pub fn clear(&mut self) {
        self.messages.clear();
        self.system_added = false;
        info!("会话已清空");
    }

    /// 获取模型名称
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 设置 UI 适配器（用于 WebSocket 等动态切换场景）
    pub fn set_ui_adapter(&mut self, ui: Arc<dyn UiAdapter>) {
        self.ui = ui;
        debug!("UI 适配器已更新");
    }

    /// 获取 UI 适配器引用（用于注册中断等外部操作）
    pub fn ui_adapter(&self) -> &Arc<dyn UiAdapter> {
        &self.ui
    }

    /// 处理中断：输出提示、清除中断标志
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

    fn trim_message_history(&mut self) {
        let system_msg_count = if self.system_added && !self.messages.is_empty() {
            1
        } else {
            0
        };

        let total_messages = self.messages.len();
        let max_non_system = self.config.session().max_message_history;

        if total_messages > system_msg_count + max_non_system {
            let keep_start = total_messages - max_non_system;
            let mut new_messages = Vec::with_capacity(system_msg_count + max_non_system);

            if system_msg_count > 0 {
                new_messages.push(self.messages[0].clone());
            }

            // 保留最近的消息
            new_messages.extend_from_slice(&self.messages[keep_start..]);

            // 添加提示消息
            new_messages.push(Message::system("[some history messages omitted to save context space]"));

            self.messages = new_messages;
            warn!("消息历史已裁剪，保留最近 {} 条消息", max_non_system);
        }
    }

    /// 执行 Reviewer Agent（带超时控制）
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

    /// 实际执行 Reviewer Agent 任务
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
            // 检查中断
            if self.ui.is_interrupted() {
                let _ = task.fail("user interrupted");
                self.handle_interrupt().await;
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
                            tracing::debug!("Skipping agent tool call: {}", tool_name);
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

                        tracing::debug!("Tool result: {} bytes", result.len());

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

        debug!("Task {} status: {:?}", task.id, task.status);
        "Analysis complete".to_string()
    }
}
