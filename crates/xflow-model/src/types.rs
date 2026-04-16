//! 消息和响应类型定义

use serde::{Deserialize, Serialize};

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// 工具调用（发送给模型时使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 工具类型
    #[serde(rename = "type")]
    pub call_type: String,
    /// 函数调用信息
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// 函数名称
    pub name: String,
    /// 参数（JSON 对象）
    pub arguments: serde_json::Value,
}

/// 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 工具调用（仅 assistant 消息）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// 工具名称（仅 tool 消息，Ollama 使用 tool_name）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl Message {
    /// 创建用户消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            images: vec![],
            tool_calls: vec![],
            tool_name: None,
        }
    }

    /// 创建助手消息
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(content.into()),
            images: vec![],
            tool_calls: vec![],
            tool_name: None,
        }
    }

    /// 创建助手消息（带工具调用）
    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: None,
            images: vec![],
            tool_calls,
            tool_name: None,
        }
    }

    /// 创建系统消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            images: vec![],
            tool_calls: vec![],
            tool_name: None,
        }
    }

    /// 创建工具结果消息
    pub fn tool_result(tool_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            images: vec![],
            tool_calls: vec![],
            tool_name: Some(tool_name.into()),
        }
    }

}

/// 流式响应块
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub reasoning: Option<String>,
    pub done: bool,
    pub tool_calls: Vec<ToolCall>,
}

/// 工具定义（用于告诉模型可用工具）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具类型，固定为 "function"
    #[serde(rename = "type")]
    pub tool_type: String,
    /// 函数信息
    pub function: FunctionDefinition,
}

/// 函数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// 函数名称
    pub name: String,
    /// 函数描述
    pub description: String,
    /// 参数 Schema (JSON Schema)
    pub parameters: serde_json::Value,
}