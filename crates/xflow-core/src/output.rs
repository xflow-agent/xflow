//! 消息输出接口
//!
//! 用于将 Session 的输出发送到不同目标（控制台、WebSocket 等）

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
    Box::new(|msg| {
        match msg {
            OutputMessage::Content(text) => print!("{}", text),
            OutputMessage::ToolCall { name, args: _ } => {
                println!("\n[调用工具: {}]", name);
            }
            OutputMessage::ToolResult { name: _, result_size } => {
                println!("[结果: {} 字节]", result_size);
            }
            OutputMessage::LoopProgress { current, max } => {
                println!("\n── 自动执行 (第 {}/{} 轮) ──", current, max);
            }
            OutputMessage::Done { tools_called, loops } => {
                println!("\n✅ 任务完成 (共调用 {} 次工具, {} 轮循环)", tools_called, loops);
            }
            OutputMessage::Error(text) => {
                println!("\n[错误: {}]", text);
            }
        }
    })
}

/// 创建空回调（用于测试）
pub fn null_callback() -> OutputCallback {
    Box::new(|_| {})
}