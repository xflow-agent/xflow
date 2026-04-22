//! CLI 适配器实现
//!
//! 将事件渲染到终端，支持 ANSI 转义码和 Markdown 格式

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use crate::events::*;
use crate::markdown_renderer::StreamingMarkdownRenderer;
use crate::ui_adapter::{AdapterContext, UiAdapter};

/// CLI 渲染状态
struct CliRenderState {
    /// 是否已打印内容图标
    has_printed_content_icon: bool,
    /// 是否已开始输出思考内容
    has_started_thinking_content: bool,
    /// 是否在思考模式
    in_thinking_mode: bool,
    /// 是否已进入工具调用阶段
    in_tool_mode: bool,
    /// 思考内容最后一个字符是否是换行
    thinking_ends_with_newline: bool,
    /// Markdown 渲染器
    markdown_renderer: StreamingMarkdownRenderer,
}

impl CliRenderState {
    fn new() -> Self {
        Self {
            has_printed_content_icon: false,
            has_started_thinking_content: false,
            in_thinking_mode: false,
            in_tool_mode: false,
            thinking_ends_with_newline: false,
            markdown_renderer: StreamingMarkdownRenderer::new(),
        }
    }

    fn reset(&mut self) {
        self.has_printed_content_icon = false;
        self.has_started_thinking_content = false;
        self.in_thinking_mode = false;
        self.in_tool_mode = false;
        self.thinking_ends_with_newline = false;
        self.markdown_renderer.reset();
    }
}

/// CLI 适配器
pub struct CliAdapter {
    context: AdapterContext,
    state: Arc<Mutex<CliRenderState>>,
}

impl CliAdapter {
    /// 创建新的 CLI 适配器
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            context: AdapterContext::new("cli"),
            state: Arc::new(Mutex::new(CliRenderState::new())),
        })
    }

    /// 渲染输出事件到终端
    fn render_output(&self, event: OutputEvent) {
        let mut state = self.state.lock().unwrap();
        const INDENT: &str = "  ";

        match event {
            OutputEvent::ThinkingStart => {
                state.reset();
                println!();
                print!("\x1b[34m✻\x1b[0m \x1b[90m\x1b[3mThinking...\x1b[0m");
            }

            OutputEvent::ThinkingDot => {
                print!("\x1b[90m\x1b[3m.\x1b[0m");
            }

            OutputEvent::ThinkingContent { text } => {
                if !state.has_started_thinking_content {
                    println!();
                    println!();
                    print!("  \x1b[90m\x1b[3m");
                    state.has_started_thinking_content = true;
                    state.in_thinking_mode = true;
                }

                for ch in text.chars() {
                    if ch == '\n' {
                        println!("\x1b[0m");
                        state.thinking_ends_with_newline = true;
                    } else {
                        if state.thinking_ends_with_newline {
                            print!("  \x1b[90m\x1b[3m");
                            state.thinking_ends_with_newline = false;
                        }
                        print!("{}", ch);
                    }
                }
            }

            OutputEvent::Content { text } => {
                if text.is_empty() || text.trim().is_empty() {
                    return;
                }

                if !state.has_printed_content_icon {
                    if state.in_thinking_mode {
                        print!("\x1b[0m");
                        println!();
                        println!();
                        state.in_thinking_mode = false;
                        state.has_started_thinking_content = false;
                    } else {
                        println!();
                        println!();
                    }
                    print!("\x1b[35m✦\x1b[0m ");
                    state.has_printed_content_icon = true;
                }

                state
                    .markdown_renderer
                    .render_chunk(&text, &mut |rendered| {
                        print!("{}", rendered);
                    });
            }

            OutputEvent::ToolCall {
                name,
                params_display,
                ..
            } => {
                if !state.in_tool_mode {
                    if state.in_thinking_mode {
                        if state.thinking_ends_with_newline {
                            print!("\x1b[0m");
                            println!();
                            println!();
                        } else {
                            println!("\x1b[0m");
                            println!();
                        }
                        state.in_thinking_mode = false;
                        state.has_started_thinking_content = false;
                    } else {
                        println!();
                    }
                    state.in_tool_mode = true;
                }

                if params_display.is_empty() {
                    println!("  \x1b[33m🛠\x1b[0m \x1b[1;97m{}\x1b[0m", name);
                } else {
                    println!(
                        "  \x1b[33m🛠\x1b[0m \x1b[1;97m{}\x1b[0m \x1b[90m{}\x1b[0m",
                        name, params_display
                    );
                }
            }

            OutputEvent::ToolResult { name: _, result } => {
                // 显示结果内容
                let content = match &result.display {
                    ToolResultDisplay::Full { content } => content.clone(),
                    ToolResultDisplay::Summary { text } => text.clone(),
                    ToolResultDisplay::LineCount { lines, preview } => {
                        format!("{} ({} lines)", preview, lines)
                    }
                    ToolResultDisplay::ByteSize { size } => format!("({})", size),
                    ToolResultDisplay::StatusOnly => String::new(),
                };

                if !content.is_empty() {
                    let display_content = if content.chars().count() > 500 {
                        format!("{}...", content.chars().take(500).collect::<String>())
                    } else {
                        content
                    };

                    for line in display_content.lines() {
                        println!("    \x1b[90m{}\x1b[0m", line);
                    }
                }

                // 显示状态
                let size_str = match &result.display {
                    ToolResultDisplay::ByteSize { size } => size.clone(),
                    _ => {
                        if result.size > 1024 {
                            format!("{:.1}KB", result.size as f64 / 1024.0)
                        } else {
                            format!("{}B", result.size)
                        }
                    }
                };

                if result.success {
                    println!(
                        "    \x1b[32m✓\x1b[0m \x1b[97m OK\x1b[90m ({})\x1b[0m",
                        size_str
                    );
                } else {
                    println!(
                        "    \x1b[31m✗\x1b[0m \x1b[97m FAILED\x1b[90m ({})\x1b[0m",
                        size_str
                    );
                }
            }

            OutputEvent::Error { message } => {
                println!("\x1b[90m{}\x1b[31m✗\x1b[90m {}\x1b[0m", INDENT, message);
            }

            OutputEvent::Done { .. } => {
                state.markdown_renderer.flush(&mut |rendered| {
                    print!("{}", rendered);
                });
                println!();
                println!();
            }

            OutputEvent::LoopProgress { .. } => {
                // 不显示循环进度
            }
        }

        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    /// 处理确认请求
    async fn handle_confirm(&self, req: ConfirmationRequest) -> UserResponse {
        println!();

        // 显示危险等级
        if req.danger_level > 0 {
            let level_display = match req.danger_level {
                3 => "CRITICAL",
                2 => "HIGH RISK",
                1 => "MODERATE RISK",
                _ => "CAUTION",
            };
            if let Some(ref reason) = req.danger_reason {
                println!("    \x1b[33m⚠️  {} - {}\x1b[0m", level_display, reason);
            } else {
                println!("    \x1b[33m⚠️  {}\x1b[0m", level_display);
            }
        }

        // 显示详情
        println!("  \x1b[90mTool:\x1b[0m {}", req.tool);
        if !req.message.is_empty() {
            for line in req.message.lines() {
                println!("  \x1b[90m{}\x1b[0m", line);
            }
        }

        // 确认提示
        let confirm_msg = if req.danger_level > 0 {
            "Confirm this dangerous operation?"
        } else {
            "Execute this operation?"
        };

        let render_config = inquire::ui::RenderConfig::default()
            .with_prompt_prefix(inquire::ui::Styled::new("    "));

        let approved = match inquire::Confirm::new(confirm_msg)
            .with_default(true)
            .with_render_config(render_config)
            .prompt()
        {
            Ok(true) => {
                println!("    \x1b[32m✓ Executing...\x1b[0m");
                true
            }
            Ok(false) => {
                println!("    \x1b[33m✗ Cancelled\x1b[0m");
                false
            }
            Err(e) => {
                tracing::warn!("Confirm dialog error: {}", e);
                println!("    \x1b[31m✗ Confirmation failed, cancelled\x1b[0m");
                false
            }
        };

        UserResponse::Confirm {
            id: req.id,
            approved,
        }
    }
}

#[async_trait]
impl UiAdapter for CliAdapter {
    async fn emit(&self, event: XflowEvent) {
        match event {
            XflowEvent::Output(output) => self.render_output(output),
            XflowEvent::Interaction(_) => {
                // 交互请求通过 request 方法处理
            }
            XflowEvent::State(_) => {
                // 状态事件在 CLI 中通常不需要特殊处理
            }
        }
    }

    async fn request(&self, request: InteractionRequest) -> Option<UserResponse> {
        match request {
            InteractionRequest::Confirm(req) => Some(self.handle_confirm(req).await),
            InteractionRequest::Input { prompt } => {
                let text = inquire::Text::new(&prompt).prompt().unwrap_or_default();
                Some(UserResponse::Input { text })
            }
            InteractionRequest::Select { options, prompt } => {
                let index = match inquire::Select::new(&prompt, options).prompt() {
                    Ok(_) => 0,
                    Err(_) => 0,
                };
                Some(UserResponse::Select { index })
            }
        }
    }

    fn is_interrupted(&self) -> bool {
        self.context.is_interrupted()
    }

    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }

    fn interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }

    fn clear_interrupt(&self) {
        self.context.clear_interrupt();
    }

    fn create_child(&self, name: &str) -> Arc<dyn UiAdapter> {
        Arc::new(ChildCliAdapter {
            context: self.context.child(name),
            state: self.state.clone(),
        })
    }
}

/// 子 CLI 适配器（用于 SubAgent）
struct ChildCliAdapter {
    context: AdapterContext,
    state: Arc<Mutex<CliRenderState>>,
}

#[async_trait]
impl UiAdapter for ChildCliAdapter {
    async fn emit(&self, event: XflowEvent) {
        // 子上下文只转发事件，不处理交互
        if let XflowEvent::Output(OutputEvent::ToolCall { name, .. }) = event {
            // 简化渲染，只输出关键信息
            println!("  \x1b[90m[Agent calling tool: {}]\x1b[0m", name);
        }
    }

    async fn request(&self, _request: InteractionRequest) -> Option<UserResponse> {
        // 子上下文不处理交互请求
        None
    }

    fn is_interrupted(&self) -> bool {
        self.context.is_interrupted()
    }

    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }

    fn interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }

    fn clear_interrupt(&self) {
        self.context.clear_interrupt();
    }

    fn create_child(&self, name: &str) -> Arc<dyn UiAdapter> {
        Arc::new(ChildCliAdapter {
            context: self.context.child(name),
            state: self.state.clone(),
        })
    }
}
