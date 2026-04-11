//! 消息和响应类型定义

use serde::{Deserialize, Serialize};

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    pub content: String,
}

impl Message {
    /// 创建用户消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            images: vec![],
        }
    }

    /// 创建助手消息
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: vec![],
        }
    }

    /// 创建系统消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            images: vec![],
        }
    }
}

/// 模型响应
#[derive(Debug, Clone)]
pub struct Response {
    pub content: String,
    pub model: String,
    pub done: bool,
}

/// 流式响应块
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub done: bool,
}

// === Ollama API 类型 ===

/// Ollama 请求
#[derive(Debug, Serialize)]
pub struct OllamaRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
}

/// Ollama 响应
#[derive(Debug, Deserialize)]
pub struct OllamaResponse {
    pub model: String,
    pub message: Option<OllamaMessage>,
    pub done: bool,
}

/// Ollama 消息
#[derive(Debug, Deserialize)]
pub struct OllamaMessage {
    pub role: String,
    pub content: String,
}

/// Ollama 流式响应
#[derive(Debug, Deserialize)]
pub struct OllamaStreamResponse {
    pub model: String,
    pub message: Option<OllamaMessage>,
    pub done: bool,
}
