//! xflow 模型接口层
//!
//! 提供统一的模型接口，支持多种后端

mod openai;
mod types;

pub use openai::OpenAIProvider;
pub use types::*;

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// 模型提供者 Trait
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// 流式发送消息（带工具支持）
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

    /// 获取模型信息
    fn model_info(&self) -> ModelInfo;
}

/// 模型信息
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("模型错误: {0}")]
    Model(String),

    #[error("流式响应解析错误: {0}")]
    StreamParse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
