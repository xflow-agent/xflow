//! Coder Agent - 编码
//!
//! 负责代码编写、修改、重构等任务

use crate::agent::{Agent, AgentContext, AgentResponse, AgentType, Task, TaskType, ToolCallRequest};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};
use xflow_model::{Message, ModelProvider, ToolDefinition};

/// Coder Agent - 代码编写器
pub struct CoderAgent {
    /// 可用工具定义
    tool_definitions: Vec<ToolDefinition>,
}

impl CoderAgent {
    /// 创建新的 Coder Agent
    pub fn new() -> Self {
        Self {
            tool_definitions: Self::build_tool_definitions(),
        }
    }

    /// 构建工具定义
    fn build_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                tool_type: "function".to_string(),
                function: xflow_model::FunctionDefinition {
                    name: "read_file".to_string(),
                    description: "读取文件内容".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "文件绝对路径"
                            }
                        },
                        "required": ["path"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: xflow_model::FunctionDefinition {
                    name: "write_file".to_string(),
                    description: "写入文件内容（需要确认）".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "文件绝对路径"
                            },
                            "content": {
                                "type": "string",
                                "description": "文件内容"
                            }
                        },
                        "required": ["path", "content"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: xflow_model::FunctionDefinition {
                    name: "search_file".to_string(),
                    description: "搜索代码内容".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": {
                                "type": "string",
                                "description": "搜索模式"
                            },
                            "path": {
                                "type": "string",
                                "description": "搜索路径"
                            }
                        },
                        "required": ["pattern"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: xflow_model::FunctionDefinition {
                    name: "run_shell".to_string(),
                    description: "执行 Shell 命令（需要确认）".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "Shell 命令"
                            }
                        },
                        "required": ["command"]
                    }),
                },
            },
        ]
    }
}

impl Default for CoderAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for CoderAgent {
    fn agent_type(&self) -> AgentType {
        AgentType::Coder
    }

    fn name(&self) -> &str {
        "coder"
    }

    fn description(&self) -> &str {
        "代码编写器，负责创建、修改、重构代码"
    }

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.task_type, TaskType::Coding | TaskType::Simple)
    }

    fn system_prompt(&self) -> String {
        r#"你是一个专业的程序员 Agent，负责编写和修改代码。

## 你的职责

1. **创建代码**: 根据需求创建新文件和代码
2. **修改代码**: 修改现有代码，修复 bug，优化性能
3. **重构代码**: 改善代码结构，提高可读性和可维护性

## 可用工具

- `read_file`: 读取文件内容
- `write_file`: 写入文件（需确认）
- `search_file`: 搜索代码
- `run_shell`: 执行命令（需确认）

## 编码规范

1. **清晰命名**: 使用有意义的变量和函数名
2. **适当注释**: 复杂逻辑添加注释
3. **错误处理**: 考虑边界情况和错误
4. **代码风格**: 遵循项目现有风格

## 工作流程

1. 先读取相关文件了解上下文
2. 分析需要修改的内容
3. 编写/修改代码
4. 验证修改是否正确

## 注意事项

- 修改前先备份或了解原代码
- 小步修改，逐步验证
- 保持代码风格一致
- 考虑向后兼容

请开始处理编码任务。"#
        .to_string()
    }

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse> {
        info!("Coder 开始处理任务: {}", task.description);
        
        // 构建消息
        let system_prompt = self.system_prompt();
        let mut messages = vec![Message::system(&system_prompt)];
        
        // 添加上下文
        let context_info = build_context_info(context);
        let previous_info = build_previous_results_info(context);
        
        let user_prompt = format!(
            "{}{}\n\n请完成以下任务：\n{}",
            context_info, previous_info, task.description
        );
        messages.push(Message::user(&user_prompt));

        // 调用模型（带工具）
        let stream = provider
            .chat_stream_with_tools(messages.clone(), self.tool_definitions.clone())
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
                        print!("{}", chunk.content);
                        full_response.push_str(&chunk.content);
                    }
                    
                    // 收集工具调用
                    for tc in chunk.tool_calls {
                        tool_calls.push(ToolCallRequest {
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        });
                    }
                    
                    if chunk.done {
                        println!();
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("模型调用失败: {}", e));
                }
            }
        }

        debug!("Coder 生成 {} 个工具调用", tool_calls.len());
        
        Ok(AgentResponse {
            output: full_response,
            success: true,
            subtasks: vec![],
            tool_calls,
            next_steps: vec![],
        })
    }
}

/// 构建上下文信息
fn build_context_info(context: &AgentContext) -> String {
    let mut info = String::new();
    
    if let Some(ref lang) = context.language {
        info.push_str(&format!("项目语言: {}\n", lang));
    }
    
    if !context.relevant_files.is_empty() {
        info.push_str("相关文件:\n");
        for file in &context.relevant_files {
            info.push_str(&format!("  - {}\n", file));
        }
    }
    
    info
}

/// 构建之前结果的信息
fn build_previous_results_info(context: &AgentContext) -> String {
    if context.previous_results.is_empty() {
        return String::new();
    }
    
    let mut info = String::from("\n之前步骤的结果:\n");
    for (i, result) in context.previous_results.iter().enumerate() {
        // 截取前 200 字符
        let preview: String = result.output.chars().take(200).collect();
        info.push_str(&format!("{}. {}\n", i + 1, preview));
    }
    
    info
}
