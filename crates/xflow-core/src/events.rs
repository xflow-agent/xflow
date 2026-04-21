//! 统一事件模型
//!
//! 定义所有 UI 交互的事件类型，用于替代原有的 Interaction + OutputCallback 双轨制

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 核心事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum XflowEvent {
    /// 输出事件（流式内容）
    Output(OutputEvent),
    /// 交互请求（需要用户响应）
    Interaction(InteractionRequest),
    /// 状态变更
    State(StateEvent),
}

/// 输出事件 - 向用户展示的内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputEvent {
    ThinkingStart,
    ThinkingDot,
    ThinkingContent { text: String },
    /// 正式回复内容（流式）
    Content { text: String },
    /// 工具调用
    ToolCall {
        name: String,
        params_display: String,
        args: Value,
    },
    /// 工具结果
    ToolResult {
        name: String,
        result: ToolResultData,
    },
    /// 错误
    Error { message: String },
    /// 完成
    Done { tools_called: usize, loops: usize },
    /// 循环进度
    LoopProgress { current: usize, max: usize },
}

/// 工具结果数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultData {
    /// 完整结果（用于后续处理）
    pub full_result: String,
    /// 展示给用户的内容
    pub display: ToolResultDisplay,
    /// 结果大小（字节）
    pub size: usize,
    /// 是否成功
    pub success: bool,
}

/// 工具结果展示方式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultDisplay {
    /// 完整内容
    Full { content: String },
    /// 摘要
    Summary { text: String },
    /// 行数统计
    LineCount { lines: usize, preview: String },
    /// 字节大小
    ByteSize { size: String },
    /// 仅状态
    StatusOnly,
}

/// 交互请求 - 需要用户响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionRequest {
    /// 确认请求
    Confirm(ConfirmationRequest),
    /// 文本输入请求
    Input { prompt: String },
    /// 选择请求
    Select {
        options: Vec<String>,
        prompt: String,
    },
}

/// 确认请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    /// 唯一标识符
    pub id: String,
    /// 工具名称
    pub tool: String,
    /// 操作描述
    pub message: String,
    /// 危险等级 (0-3)
    pub danger_level: u8,
    /// 危险原因
    pub danger_reason: Option<String>,
}

impl ConfirmationRequest {
    /// 创建新的确认请求
    pub fn new(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tool: tool.into(),
            message: message.into(),
            danger_level: 0,
            danger_reason: None,
        }
    }

    /// 设置危险等级
    pub fn with_danger(mut self, level: u8, reason: impl Into<String>) -> Self {
        self.danger_level = level;
        self.danger_reason = Some(reason.into());
        self
    }
}

/// 用户响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserResponse {
    /// 确认响应
    Confirm { id: String, approved: bool },
    /// 文本输入
    Input { text: String },
    /// 选择结果
    Select { index: usize },
    /// 中断
    Interrupt { reason: String },
}

/// 状态事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateEvent {
    /// 会话开始
    SessionStart,
    /// 会话清空
    SessionCleared,
    /// 模型切换
    ModelChanged { name: String },
    /// 连接状态变更
    ConnectionStatus { connected: bool },
}

/// 中断类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptType {
    /// 用户请求中断
    UserRequested,
    /// 超时中断
    Timeout,
    /// 错误中断
    Error,
    /// 系统中断
    System,
}

/// 中断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptInfo {
    /// 中断类型
    pub interrupt_type: InterruptType,
    /// 中断原因
    pub reason: String,
}

impl InterruptInfo {
    pub fn user(reason: impl Into<String>) -> Self {
        Self {
            interrupt_type: InterruptType::UserRequested,
            reason: reason.into(),
        }
    }

    pub fn timeout(reason: impl Into<String>) -> Self {
        Self {
            interrupt_type: InterruptType::Timeout,
            reason: reason.into(),
        }
    }

    pub fn error(reason: impl Into<String>) -> Self {
        Self {
            interrupt_type: InterruptType::Error,
            reason: reason.into(),
        }
    }
}
