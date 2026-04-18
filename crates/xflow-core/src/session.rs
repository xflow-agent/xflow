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

use crate::events::*;
use crate::ui_adapter::{AutoConfirmAdapter, UiAdapter};

/// 最大工具调用循环次数
const MAX_TOOL_LOOPS: usize = 20;

/// 等待动画最大点数
const MAX_DOTS: usize = 60;

/// 最大消息历史数量（保留最近的 N 条，不包括系统消息）
const MAX_MESSAGE_HISTORY: usize = 100;

/// 工具结果最大字符数（超过则截断）
const MAX_TOOL_RESULT_SIZE: usize = 10000;

/// Agent 执行最大超时时间（秒）
const AGENT_EXECUTION_TIMEOUT: u64 = 300; // 5 分钟

/// 使用工具的元数据来格式化参数显示
fn format_tool_params(tool: &dyn xflow_tools::Tool, args: &serde_json::Value) -> String {
    tool.format_params(args)
}

/// 将工具的确认请求转换为事件的确认请求
fn convert_confirmation_request(
    tool_name: &str,
    req: xflow_tools::ToolConfirmationRequest,
) -> ConfirmationRequest {
    let mut event_req = ConfirmationRequest::new(tool_name, req.message);
    if req.danger_level > 0 {
        event_req = event_req.with_danger(
            req.danger_level,
            req.danger_reason
                .unwrap_or_else(|| format!("危险等级: {}", req.danger_level)),
        );
    }
    event_req
}

/// 系统提示词（基础部分）
const SYSTEM_PROMPT_BASE: &str = r#"你是一个智能编程助手 xflow (心流)。你可以使用工具来帮助用户完成编程任务。

## 基础工具

- read_file: 读取单个文件内容
- write_file: 写入文件（需用户确认）
- list_directory: 列出目录内容
- search_file: 搜索代码
- run_shell: 执行 Shell 命令（需用户确认）

## 高级工具（重要！）

### reviewer_agent - 项目分析工具

**当用户说以下内容时，必须调用此工具，不要直接用 read_file：**
- "分析项目"、"分析这个项目"、"分析所有功能"
- "项目架构是什么"、"项目结构"
- "了解这个项目"、"理解项目"

**示例：**
用户："分析一下这个项目的所有功能"
正确做法：调用 reviewer_agent(task="分析项目所有功能")
错误做法：直接调用 read_file(Cargo.toml) 然后输出结论

## 工作原则

1. **识别任务类型**：先判断是"分析"还是"简单操作"
2. **选择正确工具**：复杂分析任务用 reviewer_agent，简单任务用基础工具
3. **完整执行**：不要中途停止，要完成所有必要的步骤
4. **自动循环**：你会自动循环执行直到任务完全完成

## 任务类型判断

| 用户请求 | 正确做法 |
|---------|---------|
| "分析项目功能" | 调用 reviewer_agent |
| "读取 main.rs" | 直接调用 read_file |
| "执行 cargo build" | 直接调用 run_shell |

## 重要提醒

- ❌ 不要在用户说"分析项目"时，只读取 Cargo.toml 就输出结论
- ✅ 必须调用 reviewer_agent 工具执行完整分析流程
- ❌ 不要只建议用户做什么，而是直接执行
- ✅ 完成所有必要的步骤后再输出结果"#;

/// 会话状态 V2
pub struct Session {
    /// 消息历史
    messages: Vec<Message>,
    /// 模型提供者
    provider: Arc<dyn ModelProvider>,
    /// 工作目录
    workdir: PathBuf,
    /// 模型名称
    model_name: String,
    /// 工具注册表
    tools: ToolRegistry,
    /// UI 适配器
    ui: Arc<dyn UiAdapter>,
    /// 是否已添加系统提示词
    system_added: bool,
    /// 项目上下文信息（可选）
    project_context: Option<String>,
}

impl Session {
    /// 创建新会话
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
                format!("{}\n{}", SYSTEM_PROMPT_BASE, context)
            } else {
                SYSTEM_PROMPT_BASE.to_string()
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

            if loop_count >= MAX_TOOL_LOOPS {
                warn!("达到最大工具调用循环次数：{}", MAX_TOOL_LOOPS);
                self.ui
                    .output(OutputEvent::Error {
                        message: format!("已达到最大循环次数 ({}), 停止自动执行", MAX_TOOL_LOOPS),
                    })
                    .await;
                break;
            }

            // 显示循环进度
            if loop_count > 0 {
                self.ui
                    .output(OutputEvent::LoopProgress {
                        current: loop_count + 1,
                        max: MAX_TOOL_LOOPS,
                    })
                    .await;
            }

            // 发送思考中状态
            self.ui.output(OutputEvent::ThinkingStart).await;

            // 启动等待动画 (使用 tokio::task 替代 thread::spawn)
            let animation_running = Arc::new(AtomicBool::new(true));
            let animation_task = {
                let running = animation_running.clone();
                tokio::spawn(async move {
                    let mut interval =
                        tokio::time::interval(std::time::Duration::from_millis(1000));
                    let mut dot_count = 0;
                    loop {
                        interval.tick().await;
                        if !running.load(Ordering::Relaxed) {
                            break;
                        }
                        if dot_count < MAX_DOTS {
                            print!("\x1b[90m\x1b[3m.\x1b[0m");
                            std::io::Write::flush(&mut std::io::stdout()).ok();
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
                                self.ui
                                    .output(OutputEvent::ThinkingContent {
                                        text: reasoning_text.clone(),
                                    })
                                    .await;
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
                            "请求超时。可能原因：\n  - 网络连接不稳定\n  - API 服务器响应缓慢\n  - 请求上下文过大\n".to_string()
                        } else if e.to_string().contains("connection") {
                            "连接错误。请检查：\n  - API 服务器是否正常运行\n  - 网络连接是否正常\n  - Base URL 配置是否正确\n".to_string()
                        } else {
                            format!("请求失败：{}", e)
                        };

                        self.ui
                            .output(OutputEvent::Error {
                                message: error_msg.clone(),
                            })
                            .await;

                        // 询问是否重试
                        let retry_req = ConfirmationRequest::new("retry", &error_msg);
                        if self.ui.confirm(retry_req.with_danger(0, "")).await {
                            self.ui
                                .output(OutputEvent::Content {
                                    text: "\n重新尝试...\n".to_string(),
                                })
                                .await;
                            loop_count += 1;
                            if loop_count >= MAX_TOOL_LOOPS {
                                self.ui
                                    .output(OutputEvent::Error {
                                        message: "已达到最大重试次数，请检查网络或服务器配置"
                                            .to_string(),
                                    })
                                    .await;
                                return Err(anyhow::anyhow!("达到最大重试次数"));
                            }
                            continue;
                        } else {
                            self.ui
                                .output(OutputEvent::Content {
                                    text: "\n操作已取消。输入新命令继续。".to_string(),
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
                        ("操作已取消".to_string(), false)
                    } else if tool_name == "reviewer_agent" {
                        (self.execute_reviewer_agent(tool_args.clone()).await, true)
                    } else if let Some(ref tool) = tool {
                        match tool.execute(tool_args.clone()).await {
                            Ok(result) => (result, true),
                            Err(e) => (format!("工具执行错误：{}", e), false),
                        }
                    } else {
                        (format!("未知工具：{}", tool_name), false)
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

                    debug!("工具结果：{} bytes", result.len());

                    // 截断过大的工具结果
                    let truncated_result = if result.len() > MAX_TOOL_RESULT_SIZE {
                        format!(
                            "{}\n\n[结果已截断，原始大小: {} 字符]",
                            &result[..MAX_TOOL_RESULT_SIZE],
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
            .unwrap_or("用户中断");

        warn!("执行被中断：{}", reason);

        self.ui
            .output(OutputEvent::Error {
                message: format!("执行已中断：{}", reason),
            })
            .await;

        self.ui.clear_interrupt();
    }

    /// 裁剪消息历史，保留最近的 MAX_MESSAGE_HISTORY 条非系统消息
    fn trim_message_history(&mut self) {
        // 保留系统消息（第一条如果是系统消息）
        let system_msg_count = if self.system_added && !self.messages.is_empty() {
            1 // 假设系统消息在最前面
        } else {
            0
        };

        let total_messages = self.messages.len();
        let max_non_system = MAX_MESSAGE_HISTORY;

        if total_messages > system_msg_count + max_non_system {
            // 需要裁剪，保留系统消息和最近的 max_non_system 条消息
            let keep_start = total_messages - max_non_system;
            let mut new_messages = Vec::with_capacity(system_msg_count + max_non_system);

            // 保留系统消息
            if system_msg_count > 0 {
                new_messages.push(self.messages[0].clone());
            }

            // 保留最近的消息
            new_messages.extend_from_slice(&self.messages[keep_start..]);

            // 添加提示消息
            new_messages.push(Message::system("[部分历史消息已省略以节省上下文空间]"));

            self.messages = new_messages;
            warn!("消息历史已裁剪，保留最近 {} 条消息", max_non_system);
        }
    }

    /// 执行 Reviewer Agent（带超时控制）
    async fn execute_reviewer_agent(&self, args: serde_json::Value) -> String {
        let task_desc = args
            .get("task")
            .and_then(|t| t.as_str())
            .unwrap_or("分析项目");

        info!("执行 Reviewer Agent: {}", task_desc);
        self.ui
            .output(OutputEvent::Content {
                text: "\n📊 开始项目分析...\n".to_string(),
            })
            .await;

        // 使用 timeout 包装整个 Agent 执行
        match tokio::time::timeout(
            std::time::Duration::from_secs(AGENT_EXECUTION_TIMEOUT),
            self.run_reviewer_agent_task(task_desc),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!("Agent 执行超时");
                self.ui
                    .output(OutputEvent::Error {
                        message: "项目分析超时，请稍后重试".to_string(),
                    })
                    .await;
                "分析超时".to_string()
            }
        }
    }

    /// 实际执行 Reviewer Agent 任务
    async fn run_reviewer_agent_task(&self, task_desc: &str) -> String {
        let reviewer = ReviewerAgent::new();
        let task_id = format!(
            "review-{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        let mut task = Task::new(&task_id, task_desc);

        if let Err(e) = task.start() {
            warn!("任务启动失败：{}", e);
            return format!("任务启动失败：{}", e);
        }

        debug!("任务 {} 已启动", task.id);

        let mut context = AgentContext::new(self.workdir.clone());
        let max_iterations = 10;
        let mut iteration = 0;

        loop {
            // 检查中断
            if self.ui.is_interrupted() {
                let _ = task.fail("用户中断");
                self.handle_interrupt().await;
                return "分析被中断".to_string();
            }

            if iteration >= max_iterations {
                warn!("Agent 达到最大迭代次数");
                let _ = task.fail("达到最大迭代次数");
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
                        debug!("任务 {} 已完成", task.id);
                        return response.output;
                    }

                    for tool_call in &response.tool_calls {
                        let tool_name = &tool_call.name;
                        let tool_args = &tool_call.arguments;

                        if tool_name == "reviewer_agent" {
                            tracing::debug!("跳过 Agent 工具调用：{}", tool_name);
                            continue;
                        }

                        self.ui
                            .output(OutputEvent::Content {
                                text: format!("\n[Agent 调用工具：{}]", tool_name),
                            })
                            .await;

                        let result = if let Some(tool) = self.tools.get(tool_name) {
                            match tool.execute(tool_args.clone()).await {
                                Ok(r) => r,
                                Err(e) => format!("错误：{}", e),
                            }
                        } else {
                            format!("未知工具：{}", tool_name)
                        };

                        tracing::debug!("工具结果：{} 字节", result.len());

                        context.add_tool_result(xflow_agent::ToolResult {
                            name: tool_name.clone(),
                            result,
                            success: true,
                        });
                    }

                    iteration += 1;
                }
                Err(e) => {
                    warn!("Agent 执行失败：{}", e);
                    let _ = task.fail(e.to_string());
                    return format!("分析失败：{}", e);
                }
            }
        }

        debug!("任务 {} 状态：{:?}", task.id, task.status);
        "分析完成".to_string()
    }
}
