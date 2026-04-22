//! OpenAI 兼容模型提供者实现
//!
//! 支持 OpenAI、vLLM、Ollama (OpenAI 模式) 等兼容服务

use crate::types::*;
use crate::{Error, ModelInfo, ModelProvider, Result, StreamChunk};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use std::pin::Pin;
use tracing::debug;

/// OpenAI 兼容提供者
pub struct OpenAIProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    provider_name: String,
}

impl OpenAIProvider {
    /// 创建新的 OpenAI 兼容提供者
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        model: String,
        provider_name: String,
    ) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            provider_name,
        }
    }

    /// 创建 vLLM 提供者的便捷方法
    pub fn vllm(base_url: String, model: String) -> Self {
        Self::new(base_url, None, model, "vllm".to_string())
    }

    /// 创建 OpenAI 提供者的便捷方法
    pub fn openai(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.openai.com/v1".to_string(),
            Some(api_key),
            model,
            "openai".to_string(),
        )
    }

    /// 构建 chat completions URL
    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// 将消息转换为 OpenAI 格式
    fn convert_messages(&self, messages: Vec<Message>) -> Vec<OpenAIMessage> {
        messages
            .into_iter()
            .map(|msg| match msg.role {
                Role::System => OpenAIMessage {
                    role: "system".to_string(),
                    content: msg.content,
                    tool_calls: None,
                    tool_call_id: None,
                },
                Role::User => OpenAIMessage {
                    role: "user".to_string(),
                    content: msg.content,
                    tool_calls: None,
                    tool_call_id: None,
                },
                Role::Assistant => OpenAIMessage {
                    role: "assistant".to_string(),
                    content: msg.content,
                    tool_calls: if msg.tool_calls.is_empty() {
                        None
                    } else {
                        Some(
                            msg.tool_calls
                                .into_iter()
                                .map(|tc| OpenAIToolCall {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    call_type: "function".to_string(),
                                    function: OpenAIFunctionCall {
                                        name: tc.function.name,
                                        arguments: tc.function.arguments.to_string(),
                                    },
                                })
                                .collect(),
                        )
                    },
                    tool_call_id: None,
                },
                Role::Tool => OpenAIMessage {
                    role: "tool".to_string(),
                    content: msg.content,
                    tool_calls: None,
                    tool_call_id: msg.tool_name,
                },
            })
            .collect()
    }

    /// 将工具定义转换为 OpenAI 格式
    fn convert_tools(&self, tools: Vec<ToolDefinition>) -> Vec<OpenAITool> {
        tools
            .into_iter()
            .map(|tool| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunctionDefinition {
                    name: tool.function.name,
                    description: tool.function.description,
                    parameters: tool.function.parameters,
                },
            })
            .collect()
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> {
        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: self.convert_messages(messages),
            stream: true,
            tools: self.convert_tools(tools),
            tool_choice: None,
        };

        let client = self.client.clone();
        let url = self.chat_url();
        let api_key = self.api_key.clone();
        let provider_name = self.provider_name.clone();

        let stream = async_stream::stream! {
            let builder = client.post(&url).json(&request);
            let response = match build_request_with_auth(builder, &api_key).send().await {
                Ok(r) => r,
                Err(e) => {
                    // 提供更详细的超时错误信息
                    if e.is_timeout() {
                        debug!("{} API 请求超时，请检查：\n- 网络连接是否正常\n- API 服务器是否运行\n- 请求上下文是否过大\n- 模型是否正在加载", provider_name);
                        yield Err(Error::Http(e));
                    } else {
                        debug!("{} API 请求失败：{}", provider_name, e);
                        yield Err(Error::Http(e));
                    }
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let error_body = response.text().await.unwrap_or_default();
                debug!("{} API 错误响应: {}", provider_name, error_body);
                yield Err(Error::Model(format!(
                    "{} 返回错误状态 {}: {}",
                    provider_name,
                    status,
                    error_body
                )));
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            
            // 累积工具调用（增量式）
            let mut pending_tool_calls: std::collections::HashMap<usize, (String, String, String)> = std::collections::HashMap::new();
            
            // 用于处理跨块的 <think> 标签
            let mut in_thinking = false;
            let mut thinking_buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(Error::Http(e));
                        continue;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // 处理 SSE 格式
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.trim().is_empty() || line.trim() == "data: [DONE]" {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<OpenAIStreamResponse>(data) {
                            Ok(resp) => {
                                let choice = match resp.choices.into_iter().next() {
                                    Some(c) => c,
                                    None => continue,
                                };

                                // 处理文本内容
                                let mut content = choice.delta.content.unwrap_or_default();
                                // 处理思考内容 (支持 reasoning_content 和 reasoning 两种字段名)
                                let mut reasoning = choice.delta.reasoning_content
                                    .or(choice.delta.reasoning)
                                    .filter(|s| !s.is_empty());
                                
                                // 注意：不对每个流式chunk执行trim操作，因为会导致单词间的空格被移除
                                // 例如：先返回"The"，然后返回" user"，第二个chunk开头的空格必须保留
                                // 我们只在最终的完整内容上执行trim操作
                                
                                // 处理 minimax 模型的 <think> 标签
                                if !content.is_empty() {
                                    let mut i = 0;
                                    let mut processed_content = String::new();
                                    let mut processed_reasoning = None;
                                    
                                    while i < content.len() {
                                        if in_thinking {
                                            // 寻找结束标签
                                            if let Some(end_idx) = content[i..].find("</think>") {
                                                // 提取标签内的内容
                                                thinking_buffer.push_str(&content[i..i+end_idx]);
                                                // 移除前面的 > 符号和空白
                                                let trimmed_thinking = thinking_buffer.trim_start().trim_start_matches('>').trim_start().to_string();
                                                if !trimmed_thinking.is_empty() {
                                                    processed_reasoning = Some(trimmed_thinking);
                                                }
                                                // 重置状态
                                                in_thinking = false;
                                                thinking_buffer.clear();
                                                // 跳过结束标签
                                                i += end_idx + 7;
                                                
                                                // 跳过结束标签后的 > 符号和空白
                                                while i < content.len() && (content[i..].starts_with('>') || content[i..].starts_with('\n') || content[i..].starts_with(' ')) {
                                                    i += 1;
                                                }
                                            } else {
                                                // 没有找到结束标签，将剩余内容添加到思考缓冲区
                                                thinking_buffer.push_str(&content[i..]);
                                                i = content.len();
                                            }
                                        } else {
                                            // 寻找开始标签
                                            if let Some(start_idx) = content[i..].find("<think>") {
                                                // 将标签前的内容添加到正文
                                                let pre_think_content = &content[i..i+start_idx];
                                                processed_content.push_str(pre_think_content);
                                                // 开始思考模式
                                                in_thinking = true;
                                                // 跳过开始标签
                                                i += start_idx + 6;
                                            } else {
                                                // 没有找到开始标签，将剩余内容添加到正文
                                                processed_content.push_str(&content[i..]);
                                                i = content.len();
                                            }
                                        }
                                    }
                                    
                                    // 更新内容和思考
                                    content = processed_content;
                                    if processed_reasoning.is_some() {
                                        reasoning = processed_reasoning;
                                    }
                                }

                                // 实时输出内容，确保流式体验
                                // 移除内容和思考末尾的换行，确保所有模型的输出格式一致
                                let trimmed_content = content.trim_end_matches('\n').to_string();
                                let trimmed_reasoning = reasoning.map(|r| r.trim_end_matches('\n').to_string());
                                
                                if !trimmed_content.is_empty() || trimmed_reasoning.is_some() {
                                    yield Ok(StreamChunk {
                                        content: trimmed_content,
                                        reasoning: trimmed_reasoning,
                                        done: false,
                                        tool_calls: vec![],
                                    });
                                }

                                // 处理工具调用（增量式累积）
                                if let Some(tool_calls) = choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        let idx = tc.index as usize;
                                        let entry = pending_tool_calls.entry(idx).or_insert((
                                            String::new(),  // id
                                            String::new(),  // name
                                            String::new(),  // arguments
                                        ));

                                        // 累积各部分
                                        if let Some(id) = tc.id {
                                            if !id.is_empty() {
                                                entry.0 = id;
                                            }
                                        }
                                        if let Some(ref func) = tc.function {
                                            if let Some(name) = &func.name {
                                                if !name.is_empty() {
                                                    entry.1 = name.clone();
                                                }
                                            }
                                            if let Some(args) = &func.arguments {
                                                entry.2.push_str(args);
                                            }
                                        }
                                    }
                                }

                                // 检查是否完成，完成时输出累积的工具调用
                                if choice.finish_reason.is_some() {
                                    // 将累积的工具调用转换为 ToolCall
                                    let converted: Vec<ToolCall> = pending_tool_calls
                                        .values()
                                        .filter(|(_, name, _)| !name.is_empty())
                                        .map(|(_id, name, args)| {
                                            ToolCall {
                                                call_type: "function".to_string(),
                                                function: FunctionCall {
                                                    name: name.clone(),
                                                    arguments: serde_json::from_str(args)
                                                        .unwrap_or_else(|_| {
                                                            // 如果解析失败，尝试作为字符串处理
                                                            serde_json::Value::String(args.clone())
                                                        }),
                                                },
                                            }
                                        })
                                        .collect();

                                    // 输出工具调用，不重复输出内容
                                    yield Ok(StreamChunk {
                                        content: String::new(),
                                        reasoning: None,
                                        done: true,
                                        tool_calls: converted,
                                    });

                                    pending_tool_calls.clear();
                                }
                            }
                            Err(e) => {
                                // 忽略解析错误，可能是不完整的 JSON
                                debug!("SSE 解析错误: {} - 数据: {}", e, data);
                            }
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
            provider: self.provider_name.clone(),
        }
    }
}

/// 辅助函数：构建带认证的请求
fn build_request_with_auth(
    builder: reqwest::RequestBuilder,
    api_key: &Option<String>,
) -> reqwest::RequestBuilder {
    if let Some(key) = api_key {
        builder.header("Authorization", format!("Bearer {}", key))
    } else {
        builder
    }
}

// ============================================================================
// OpenAI API 类型定义
// ============================================================================

/// OpenAI Chat 请求
#[derive(Debug, serde::Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

/// OpenAI 消息格式
#[derive(Debug, serde::Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// OpenAI 工具定义
#[derive(Debug, serde::Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunctionDefinition,
}

/// OpenAI 函数定义
#[derive(Debug, serde::Serialize)]
struct OpenAIFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// OpenAI 工具调用
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunctionCall,
}

/// OpenAI 函数调用
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

/// OpenAI 响应
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIResponse {
    model: String,
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

/// OpenAI 选择项
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIChoice {
    index: u32,
    message: OpenAIResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

/// OpenAI 响应消息
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIResponseMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

/// OpenAI 用量统计
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// OpenAI 流式响应
#[derive(Debug, serde::Deserialize)]
struct OpenAIStreamResponse {
    #[serde(default)]
    choices: Vec<OpenAIStreamChoice>,
}

/// OpenAI 流式选择项
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIStreamChoice {
    index: u32,
    delta: OpenAIDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

/// OpenAI 增量内容
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

/// OpenAI 流式工具调用
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIStreamToolCall {
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamFunctionCall>,
}

/// OpenAI 流式函数调用
#[derive(Debug, serde::Deserialize)]
struct OpenAIStreamFunctionCall {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::vllm(
            "http://localhost:8000/v1".to_string(),
            "llama-3-70b".to_string(),
        );
        assert_eq!(provider.model_info().provider, "vllm");
        assert_eq!(provider.model_info().name, "llama-3-70b");
    }

    #[test]
    fn test_openai_provider_with_key() {
        let provider = OpenAIProvider::openai("sk-test".to_string(), "gpt-4".to_string());
        assert_eq!(provider.model_info().provider, "openai");
        assert_eq!(provider.model_info().name, "gpt-4");
    }
}
