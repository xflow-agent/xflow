//! Reviewer Agent - 代码审查
//!
//! 负责代码审查、分析、问题检测等任务

use crate::agent::{Agent, AgentContext, AgentResponse, AgentType, Task, ToolCallRequest};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;
use xflow_model::{get_reviewer_prompt, Message, ModelProvider, ToolDefinition};

/// 输出回调类型
type OutputCallback = Box<dyn Fn(String) + Send + Sync>;

/// Reviewer Agent - 代码审查器
pub struct ReviewerAgent {
    /// 可用工具定义
    tool_definitions: Vec<ToolDefinition>,
    /// 输出回调（可选）
    output_callback: Option<OutputCallback>,
}

impl ReviewerAgent {
    /// 创建新的 Reviewer Agent
    pub fn new(tool_definitions: Vec<ToolDefinition>) -> Self {
        Self {
            tool_definitions,
            output_callback: None,
        }
    }

    /// 创建带输出回调的 Reviewer Agent
    pub fn with_output(tool_definitions: Vec<ToolDefinition>, output_callback: OutputCallback) -> Self {
        Self {
            tool_definitions,
            output_callback: Some(output_callback),
        }
    }

    /// 设置输出回调
    pub fn set_output_callback(&mut self, output_callback: OutputCallback) {
        self.output_callback = Some(output_callback);
    }
}

impl Default for ReviewerAgent {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[async_trait]
impl Agent for ReviewerAgent {
    fn agent_type(&self) -> AgentType {
        AgentType::Reviewer
    }

    fn name(&self) -> &str {
        "reviewer"
    }

    fn description(&self) -> &str {
        "代码审查器，负责代码分析、问题检测、质量评估"
    }

    fn system_prompt(&self) -> String {
        get_reviewer_prompt()
    }

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse> {
        info!("Reviewer 开始审查：{}", task.description);

        // 构建消息
        let system_prompt = self.system_prompt();
        let mut messages = vec![Message::system(&system_prompt)];

        // 构建上下文信息
        let mut context_parts = Vec::new();

        // 添加相关文件
        if !context.relevant_files.is_empty() {
            context_parts.push(format!(
                "需要审查的文件:\n{}",
                context.relevant_files.join("\n")
            ));
        }

        // 检查是否有工具执行结果
        let has_tool_results = !context.tool_results.is_empty();

        // 构建用户提示
        let context_info = if context_parts.is_empty() {
            String::new()
        } else {
            context_parts.join("\n\n") + "\n\n"
        };

        let user_prompt = if has_tool_results {
            // 有工具结果时，明确告诉模型如何继续
            let tool_results = context.tool_results_summary();
            format!(
                "{}{}\n\n你已经执行了工具调用并获得了结果。\n\
                请基于上述工具执行结果，继续完成审查任务：\n{}\n\n\
                你可以：\n\
                1. 基于已有结果直接输出分析报告\n\
                2. 如果需要更多信息，可以继续调用工具\n\
                \n\
                现在请继续处理：",
                context_info, tool_results, task.description
            )
        } else {
            // 没有工具结果时，是初始调用
            format!(
                "{}审查任务:\n{}\n\n\
                你可以使用可用工具来收集信息，然后输出分析结果。",
                context_info, task.description
            )
        };

        messages.push(Message::user(&user_prompt));

        // 调用模型（带工具）
        let stream = provider
            .chat_stream(messages.clone(), self.tool_definitions.clone())
            .await;

        // 处理响应
        use futures::StreamExt;
        let mut stream = stream;
        let mut full_response = String::new();
        let mut tool_calls = Vec::new();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    if !chunk.content.is_empty() {
                        // 修复：使用输出回调而不是 print!
                        if let Some(ref callback) = self.output_callback {
                            callback(chunk.content.clone());
                        }
                        full_response.push_str(&chunk.content);
                    }

                    for tc in chunk.tool_calls {
                        tool_calls.push(ToolCallRequest {
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        });
                    }

                    if chunk.done {
                        // 修复：使用输出回调而不是 println!
                        if let Some(ref callback) = self.output_callback {
                            callback("\n".to_string());
                        }
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("模型调用失败：{}", e));
                }
            }
        }

        Ok(AgentResponse {
            output: full_response,
            success: true,
            tool_calls,
        })
    }
}
