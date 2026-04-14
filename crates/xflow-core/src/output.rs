//! 消息输出接口
//!
//! 用于将 Session 的输出发送到不同目标（控制台、WebSocket 等）

use std::sync::{Mutex, Arc};

/// 消息类型
#[derive(Debug, Clone)]
pub enum OutputMessage {
    /// 文本内容（流式）
    Content(String),
    /// 工具调用开始
    ToolCall { name: String, args: String },
    /// 工具结果
    ToolResult { name: String, result_size: usize },
    /// 循环进度
    LoopProgress { current: usize, max: usize },
    /// 任务完成
    Done { tools_called: usize, loops: usize },
    /// 错误
    Error(String),
}

/// 消息回调类型
pub type OutputCallback = Box<dyn Fn(OutputMessage) + Send + Sync>;

/// 创建控制台回调
pub fn console_callback() -> OutputCallback {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    
    let is_thinking = Arc::new(AtomicBool::new(false));
    
    Box::new(move |msg| {
        match msg {
            OutputMessage::Content(text) => {
                // 如果之前显示过思考状态，先结束它
                if is_thinking.swap(false, Ordering::SeqCst) {
                    println!();
                }
                print!("{}", text);
            }
            OutputMessage::ToolCall { name, args } => {
                println!();
                if args.is_empty() {
                    println!("\x1b[90m✻ {}\x1b[0m", name);
                } else {
                    // 折叠显示工具调用详情
                    println!("\x1b[90m✻ {}\x1b[0m", name);
                    println!("  \x1b[90m{}\x1b[0m", args);
                }
            }
            OutputMessage::ToolResult { name, result_size } => {
                // 显示文件变更统计
                let size_str = if result_size > 1024 {
                    format!("{:.1}KB", result_size as f64 / 1024.0)
                } else {
                    format!("{}B", result_size)
                };
                println!("\x1b[32m✓ {}: {}\x1b[0m", name, size_str);
            }
            OutputMessage::LoopProgress { current, max } => {
                println!("\n\x1b[90m── 第 {}/{} 轮 ──\x1b[0m", current, max);
            }
            OutputMessage::Done { tools_called, loops } => {
                if tools_called > 0 {
                    println!("\n\x1b[32m✓ 完成 ({} 个工具, {} 轮)\x1b[0m", tools_called, loops);
                }
            }
            OutputMessage::Error(text) => {
                println!("\n\x1b[31m✗ {}\x1b[0m", text);
            }
        }
    })
}

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