//! Reviewer Agent - 代码审查
//!
//! 负责代码审查、分析、问题检测等任务

use crate::agent::{Agent, AgentContext, AgentResponse, AgentType, Task, TaskType, ToolCallRequest, generate_task_id};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;
use xflow_model::{Message, ModelProvider, ToolDefinition};

/// Reviewer Agent - 代码审查器
pub struct ReviewerAgent {
    /// 可用工具定义
    tool_definitions: Vec<ToolDefinition>,
}

impl ReviewerAgent {
    /// 创建新的 Reviewer Agent
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
                    description: "执行 Shell 命令（如 cargo check, cargo clippy）".to_string(),
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

impl Default for ReviewerAgent {
    fn default() -> Self {
        Self::new()
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

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.task_type, TaskType::Review | TaskType::Analysis)
    }

    fn system_prompt(&self) -> String {
        r#"你是一个专业的代码审查员 Agent，负责检查代码质量和发现问题。

## 你的职责

1. **代码审查**: 检查代码质量、风格、潜在问题
2. **问题分析**: 分析 bug 原因、性能瓶颈
3. **安全检查**: 检查安全漏洞、敏感信息泄露
4. **架构评估**: 评估代码结构、依赖关系

## 可用工具

- `read_file`: 读取文件内容
- `search_file`: 搜索代码
- `run_shell`: 执行检查命令

## 审查清单

### 代码质量
- [ ] 代码是否清晰易懂
- [ ] 命名是否有意义
- [ ] 是否有重复代码
- [ ] 错误处理是否完善

### 潜在问题
- [ ] 空指针/空值检查
- [ ] 边界条件处理
- [ ] 并发安全问题
- [ ] 资源泄漏风险

### 性能
- [ ] 不必要的循环/计算
- [ ] 内存使用效率
- [ ] I/O 操作优化

### 安全
- [ ] 输入验证
- [ ] 敏感数据处理
- [ ] 权限检查

## 输出格式

```
审查结果摘要: [总体评价]

发现的问题:
1. [问题描述] (严重程度: 高/中/低)
   位置: [文件:行号]
   建议: [修复建议]

优点:
- [优点1]
- [优点2]

改进建议:
1. [建议1]
2. [建议2]
```

请开始审查任务。"#
        .to_string()
    }

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse> {
        info!("Reviewer 开始审查: {}", task.description);
        
        // 构建消息
        let system_prompt = self.system_prompt();
        let mut messages = vec![Message::system(&system_prompt)];
        
        // 添加上下文
        let context_info = if !context.relevant_files.is_empty() {
            format!("需要审查的文件:\n{}\n", context.relevant_files.join("\n"))
        } else {
            "需要审查整个项目".to_string()
        };
        
        let user_prompt = format!(
            "{}\n\n审查任务:\n{}",
            context_info, task.description
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
        
        Ok(AgentResponse {
            output: full_response,
            success: true,
            subtasks: vec![],
            tool_calls,
            next_steps: vec![],
        })
    }
}
