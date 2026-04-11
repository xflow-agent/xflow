//! 会话管理

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};
use xflow_model::{Message, ModelProvider, StreamChunk};

/// 会话状态
pub struct Session {
    /// 消息历史
    messages: Vec<Message>,
    /// 模型提供者
    provider: Arc<dyn ModelProvider>,
    /// 工作目录
    #[allow(dead_code)]
    workdir: PathBuf,
    /// 模型名称
    model_name: String,
}

impl Session {
    /// 创建新会话
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf) -> Self {
        let model_name = provider.model_info().name;
        Self {
            messages: Vec::new(),
            provider,
            workdir,
            model_name,
        }
    }

    /// 处理用户输入
    pub async fn process(&mut self, input: &str) -> Result<()> {
        // 添加用户消息
        self.messages.push(Message::user(input));

        debug!("当前消息数量: {}", self.messages.len());

        // 调用模型（流式）
        let stream = self.provider.chat_stream(self.messages.clone()).await;

        // 处理流式响应
        let mut full_response = String::new();
        let mut stream = stream;

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(StreamChunk { content, done }) => {
                    if !content.is_empty() {
                        // 输出内容（实时显示）
                        print!("{}", content);
                        full_response.push_str(&content);
                    }
                    if done {
                        println!(); // 换行
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("流式响应错误: {}", e);
                    println!("\n[错误: {}]", e);
                    break;
                }
            }
        }

        // 添加助手消息到历史
        if !full_response.is_empty() {
            self.messages.push(Message::assistant(&full_response));
        }

        Ok(())
    }

    /// 清空会话
    pub fn clear(&mut self) {
        self.messages.clear();
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
}
