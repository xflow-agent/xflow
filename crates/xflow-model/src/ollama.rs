//! Ollama 模型提供者实现

use crate::types::*;
use crate::{Error, ModelInfo, ModelProvider, Result, StreamChunk};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use std::pin::Pin;
use tracing::{debug, warn};

/// Ollama 提供者
pub struct OllamaProvider {
    client: Client,
    host: String,
    model: String,
}

impl OllamaProvider {
    /// 创建新的 Ollama 提供者
    pub fn new(host: String, model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, host, model }
    }

    /// 构建请求 URL
    fn url(&self, endpoint: &str) -> String {
        format!("{}/api/{}", self.host.trim_end_matches('/'), endpoint)
    }

    /// 构建系统提示
    fn build_system_message(&self) -> Message {
        Message::system(
            r#"你是 xflow (心流)，一个专业的 AI 编程助手。

你的能力包括：
- 代码编写、修改和解释
- 项目结构和依赖分析
- 调试和错误修复
- 最佳实践建议
- 读取项目文件内容

你可以使用以下工具：
- read_file: 读取文件内容，参数: {"path": "文件路径"}

当用户要求查看文件内容时，请使用 read_file 工具。
当前工作目录是用户的项目根目录。"#,
        )
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn chat(&self, messages: Vec<Message>) -> Result<Response> {
        // 添加系统消息
        let mut all_messages = vec![self.build_system_message()];
        all_messages.extend(messages);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: all_messages,
            stream: false,
            tools: vec![],
        };

        debug!("发送请求到 Ollama: {:?}", request);

        let response = self
            .client
            .post(self.url("chat"))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Model(format!(
                "Ollama 返回错误状态 {}: {}",
                status, text
            )));
        }

        let ollama_resp: OllamaResponse = response.json().await?;

        let content = ollama_resp
            .message
            .and_then(|m| m.content)
            .unwrap_or_default();

        Ok(Response {
            content,
            model: ollama_resp.model,
            done: ollama_resp.done,
            tool_calls: vec![],
        })
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> {
        self.chat_stream_with_tools(messages, vec![]).await
    }

    async fn chat_stream_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> {
        // 添加系统消息
        let mut all_messages = vec![self.build_system_message()];
        all_messages.extend(messages);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: all_messages,
            stream: true,
            tools,
        };

        let client = self.client.clone();
        let url = self.url("chat");

        // 创建流
        let stream = async_stream::stream! {
            let response = match client
                .post(&url)
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    yield Err(Error::Http(e));
                    return;
                }
            };

            if !response.status().is_success() {
                yield Err(Error::Model(format!(
                    "Ollama 返回错误状态 {}",
                    response.status()
                )));
                return;
            }

            let mut stream = response.bytes_stream();

            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(Error::Http(e));
                        continue;
                    }
                };

                let text = String::from_utf8_lossy(&chunk);

                // Ollama 每行是一个独立的 JSON
                for line in text.lines() {
                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<OllamaStreamResponse>(line) {
                        Ok(resp) => {
                            // 处理文本内容
                            let content = resp
                                .message
                                .as_ref()
                                .and_then(|m| m.content.as_ref())
                                .cloned()
                                .unwrap_or_default();

                            // 处理工具调用
                            let tool_calls = resp
                                .message
                                .map(|m| m.tool_calls)
                                .unwrap_or_default();

                            if !content.is_empty() {
                                yield Ok(StreamChunk {
                                    content,
                                    done: false,
                                    tool_calls: vec![],
                                });
                            }

                            if !tool_calls.is_empty() {
                                // 转换工具调用格式
                                let converted: Vec<ToolCall> = tool_calls
                                    .into_iter()
                                    .map(|tc| ToolCall {
                                        call_type: if tc.call_type.is_empty() {
                                            "function".to_string()
                                        } else {
                                            tc.call_type
                                        },
                                        function: FunctionCall {
                                            name: tc.function.name,
                                            arguments: tc.function.arguments,
                                        },
                                    })
                                    .collect();

                                yield Ok(StreamChunk {
                                    content: String::new(),
                                    done: false,
                                    tool_calls: converted,
                                });
                            }

                            if resp.done {
                                yield Ok(StreamChunk {
                                    content: String::new(),
                                    done: true,
                                    tool_calls: vec![],
                                });
                            }
                        }
                        Err(e) => {
                            warn!("解析流式响应失败: {} - 内容: {}", e, line);
                        }
                    }
                }
            }
        };

        Box::pin(stream)
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model.clone(),
            provider: "ollama".to_string(),
        }
    }
}