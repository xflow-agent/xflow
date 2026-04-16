//! 消息输出接口
//!
//! 用于将 Session 的输出发送到不同目标（控制台、WebSocket 等）

use std::sync::{Mutex, Arc};

/// 消息类型
#[derive(Debug, Clone)]
pub enum OutputMessage {
    /// 思考中状态开始
    Thinking,
    /// 思考/推理内容（流式，灰色斜体）
    ThinkingContent(String),
    /// 文本内容（流式）
    Content(String),
    /// 工具调用开始
    ToolCall { 
        name: String, 
        args: String,
        /// 格式化的参数显示
        params_display: String,
    },
    /// 工具结果
    ToolResult { 
        name: String, 
        result: String,
        result_size: usize,
        success: bool,
    },
    /// 循环进度
    LoopProgress { current: usize, max: usize },
    /// 任务完成
    Done { tools_called: usize, loops: usize },
    /// 错误
    Error(String),
}

/// 消息回调类型
pub type OutputCallback = Box<dyn Fn(OutputMessage) + Send + Sync>;

/// CLI 显示状态（只属于 CLI Display，不属于 Session）
struct CliDisplayState {
    /// 是否已打印内容图标（✦）
    has_printed_content_icon: bool,
    /// 是否已开始输出思考内容（用于控制第一次的换行和样式）
    has_started_thinking_content: bool,
    /// 是否在思考模式（用于判断是否需要关闭样式）
    in_thinking_mode: bool,
    /// 是否已进入工具调用阶段
    in_tool_mode: bool,
    /// 思考内容最后一个字符是否是换行
    thinking_ends_with_newline: bool,
}

impl CliDisplayState {
    fn new() -> Self {
        Self {
            has_printed_content_icon: false,
            has_started_thinking_content: false,
            in_thinking_mode: false,
            in_tool_mode: false,
            thinking_ends_with_newline: false,
        }
    }
}

/// 缩进宽度
const INDENT: &str = "  ";

/// 创建控制台回调（维护自己的显示状态）
pub fn console_callback() -> OutputCallback {
    let state = Arc::new(Mutex::new(CliDisplayState::new()));
    
    Box::new(move |msg| {
        let mut s = state.lock().unwrap();
        
        match msg {
            OutputMessage::Thinking => {
                // 重置所有状态（新的对话轮次开始）
                s.has_printed_content_icon = false;
                s.has_started_thinking_content = false;
                s.in_thinking_mode = false;
                s.in_tool_mode = false;
                s.thinking_ends_with_newline = false;
                
                // 思考中状态：蓝色✻图标 + 灰色斜体"思考中..."
                println!();
                print!("\x1b[34m✻\x1b[0m \x1b[90m\x1b[3m思考中...\x1b[0m");
            }
            
            OutputMessage::ThinkingContent(text) => {
                // 第一次输出思考内容时，换行并开始灰色斜体样式
                if !s.has_started_thinking_content {
                    println!();  // 换行，与"思考中..."分开
                    print!("  \x1b[90m\x1b[3m");  // 缩进 + 灰色斜体
                    s.has_started_thinking_content = true;
                    s.in_thinking_mode = true;
                }
                // 思考内容：灰色斜体，带缩进
                for ch in text.chars() {
                    if ch == '\n' {
                        println!("\x1b[0m"); // 关闭样式并换行
                        // 不立即开启样式，等到下一个非换行字符时开启
                        s.thinking_ends_with_newline = true;
                    } else {
                        // 如果之前是换行，现在要输出内容，开启样式
                        if s.thinking_ends_with_newline {
                            print!("  \x1b[90m\x1b[3m"); // 缩进 + 灰色斜体
                            s.thinking_ends_with_newline = false;
                        }
                        print!("{}", ch);
                    }
                }
            }
            
            OutputMessage::Content(text) => {
                // 第一次输出时打印图标
                if !s.has_printed_content_icon {
                    // 如果之前在思考模式，关闭样式
                    if s.in_thinking_mode {
                        if s.thinking_ends_with_newline {
                            // 光标在新行，样式已关闭（换行时已关闭）
                            // 思考内容的换行本身就是空行，不需要额外空行
                            print!("\x1b[0m"); // 确保样式关闭
                        } else {
                            // 光标在内容末尾，样式已开启
                            println!("\x1b[0m"); // 关闭样式并换行（这行就是空行）
                        }
                        s.in_thinking_mode = false;
                        s.has_started_thinking_content = false;
                    } else {
                        println!(); // 如果没有思考模式，输出空行
                    }
                    // 紫色✦图标
                    print!("\x1b[35m✦\x1b[0m ");
                    s.has_printed_content_icon = true;
                }
                print!("{}", text);
            }
            
            OutputMessage::ToolCall { name, args: _, params_display } => {
                // 第一次工具调用时，关闭思考模式样式
                if !s.in_tool_mode {
                    if s.in_thinking_mode {
                        if s.thinking_ends_with_newline {
                            // 光标在新行，样式已关闭（换行时已关闭）
                            // 思考内容的换行本身就是空行，不需要额外空行
                            print!("\x1b[0m"); // 确保样式关闭
                        } else {
                            // 光标在内容末尾，样式已开启
                            println!("\x1b[0m"); // 关闭样式并换行（这行就是空行）
                        }
                        s.in_thinking_mode = false;
                        s.has_started_thinking_content = false;
                    } else {
                        println!(); // 如果没有思考模式，输出空行
                    }
                    s.in_tool_mode = true;
                }
                
                // 工具调用：黄色🛠图标 + 白色工具名 + 灰色参数，带缩进
                if params_display.is_empty() {
                    println!("  \x1b[33m🛠\x1b[0m \x1b[97m{}\x1b[0m", name);
                } else {
                    println!("  \x1b[33m🛠\x1b[0m \x1b[97m{}\x1b[0m \x1b[90m{}\x1b[0m", name, params_display);
                }
            }
            
            OutputMessage::ToolResult { name: _, result, result_size, success } => {
                // 先显示结果内容（保持缩进）
                if !result.is_empty() {
                    // 截断显示，避免输出太长
                    let display_result = if result.chars().count() > 500 {
                        format!("{}...", result.chars().take(500).collect::<String>())
                    } else {
                        result.clone()
                    };
                    // 按行输出，每行保持缩进
                    for line in display_result.lines() {
                        println!("  \x1b[90m{}\x1b[0m", line);
                    }
                }
                // 显示成功/失败状态
                let size_str = if result_size > 1024 {
                    format!("{:.1}KB", result_size as f64 / 1024.0)
                } else {
                    format!("{}B", result_size)
                };
                if success {
                    println!("  \x1b[32m✓\x1b[0m \x1b[97m调用成功\x1b[90m ({})\x1b[0m", size_str);
                } else {
                    println!("  \x1b[31m✗\x1b[0m \x1b[97m调用失败\x1b[90m ({})\x1b[0m", size_str);
                }
            }
            
            OutputMessage::LoopProgress { current: _, max: _ } => {
                // 不显示循环进度
            }
            
            OutputMessage::Done { tools_called: _, loops: _ } => {
                println!();
                println!();
            }
            
            OutputMessage::Error(text) => {
                println!("\x1b[90m{}\x1b[31m✗\x1b[90m {}\x1b[0m", INDENT, text);
            }
        }
        
        std::io::Write::flush(&mut std::io::stdout()).ok();
    })
}

// /// 显示思考中状态
// pub fn print_thinking() {
//     println!("\x1b[34m✻\x1b[0m \x1b[90m\x1b[3m思考中...\x1b[0m");
//     println!();
// }

// /// 显示思考/推理内容（灰色斜体，带缩进）
// pub fn print_thinking_content(text: &str) {
//     print!("  \x1b[90m\x1b[3m{}\x1b[0m", text);
// }

/// 创建空回调（用于测试）
pub fn null_callback() -> OutputCallback {
    Box::new(|_| {})
}

/// 创建通道回调（用于实时发送）
pub fn channel_callback(tx: std::sync::mpsc::Sender<OutputMessage>) -> OutputCallback {
    Box::new(move |msg| {
        let _ = tx.send(msg);
    })
}

/// 创建队列回调（用于收集消息）
pub fn channel_callback_with_queue(queue: Arc<Mutex<Vec<OutputMessage>>>) -> OutputCallback {
    Box::new(move |msg| {
        if let Ok(mut q) = queue.lock() {
            q.push(msg);
        }
    })
}

/// 创建实时回调（用于 WebSocket 实时发送）
///
/// 使用 tokio::sync::mpsc::UnboundedSender，可以在同步回调中发送，
/// 后台任务接收后立即发送到 WebSocket。
pub fn realtime_callback(tx: tokio::sync::mpsc::UnboundedSender<OutputMessage>) -> OutputCallback {
    Box::new(move |msg| {
        // UnboundedSender::send 是非阻塞的，适合在同步回调中使用
        let _ = tx.send(msg);
    })
}