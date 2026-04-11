//! Planner Agent - 任务分解
//!
//! 负责将复杂任务分解为可执行的子任务

use crate::agent::{Agent, AgentContext, AgentResponse, AgentType, Task, TaskType, TaskStatus, generate_task_id};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};
use xflow_model::{Message, ModelProvider};

/// Planner Agent - 任务规划器
pub struct PlannerAgent;

impl PlannerAgent {
    /// 创建新的 Planner Agent
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlannerAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for PlannerAgent {
    fn agent_type(&self) -> AgentType {
        AgentType::Planner
    }

    fn name(&self) -> &str {
        "planner"
    }

    fn description(&self) -> &str {
        "任务规划器，负责将复杂任务分解为可执行的子任务序列"
    }

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.task_type, TaskType::Complex)
    }

    fn system_prompt(&self) -> String {
        r#"你是一个任务规划专家。你的职责是将用户的复杂任务分解为可执行的子任务序列。

## 分解原则

1. **原子性**: 每个子任务应该是一个独立的、可验证的操作
2. **有序性**: 子任务之间有明确的执行顺序，依赖关系清晰
3. **完整性**: 所有子任务完成后，原任务即完成
4. **合理性**: 每个子任务都可以由编码器或审查器独立完成

## 任务类型

识别并分类用户任务：
- **创建**: 创建新文件、新项目、新功能
- **修改**: 修改现有代码、修复 bug、重构
- **分析**: 分析代码、查找问题、理解逻辑
- **测试**: 编写测试、运行测试、修复测试失败

## 输出格式

请以结构化方式输出分解结果：

```
任务分析: [原任务的简要分析]

子任务列表:
1. [子任务1描述] (类型: 创建/修改/分析/测试)
2. [子任务2描述] (类型: 创建/修改/分析/测试)
...

执行顺序: 线性/并行 (说明是否可以并行执行)
预估难度: 简单/中等/复杂
```

## 示例

用户任务: "在 src 目录创建一个新的用户认证模块，包含登录、注册、登出功能"

分解结果:
```
任务分析: 需要创建用户认证模块，包含三个核心功能

子任务列表:
1. 创建 src/auth/mod.rs 模块文件 (类型: 创建)
2. 创建 src/auth/login.rs 登录功能 (类型: 创建)
3. 创建 src/auth/register.rs 注册功能 (类型: 创建)
4. 创建 src/auth/logout.rs 登出功能 (类型: 创建)
5. 在 src/lib.rs 中导出 auth 模块 (类型: 修改)

执行顺序: 线性 (有依赖关系)
预估难度: 中等
```

请开始分析用户任务并进行分解。"#
        .to_string()
    }

    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse> {
        info!("Planner 开始分析任务: {}", task.description);
        
        // 构建消息
        let system_prompt = self.system_prompt();
        let mut messages = vec![Message::system(&system_prompt)];
        
        // 添加上下文信息
        let context_info = if !context.relevant_files.is_empty() {
            format!("\n相关文件:\n{}\n", context.relevant_files.join("\n"))
        } else {
            String::new()
        };
        
        let user_prompt = format!(
            "请分解以下任务：\n\n{}\n{}",
            task.description, context_info
        );
        messages.push(Message::user(&user_prompt));

        // 调用模型
        let stream = provider.chat_stream(messages.clone()).await;
        
        // 收集响应
        use futures::StreamExt;
        let mut stream = stream;
        let mut full_response = String::new();
        
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    if !chunk.content.is_empty() {
                        print!("{}", chunk.content);
                        full_response.push_str(&chunk.content);
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

        // 解析分解结果，提取子任务
        let subtasks = parse_subtasks(&full_response);
        info!("分解出 {} 个子任务", subtasks.len());
        
        Ok(AgentResponse {
            output: full_response,
            success: true,
            subtasks,
            tool_calls: vec![],
            next_steps: vec!["执行分解后的子任务".to_string()],
        })
    }
}

/// 解析模型输出中的子任务
fn parse_subtasks(response: &str) -> Vec<Task> {
    let mut tasks = Vec::new();
    
    // 简单的行解析：查找 "1. ", "2. " 等开头的行
    for line in response.lines() {
        let trimmed = line.trim();
        
        // 检查是否是任务行（数字. 开头）
        if let Some(pos) = trimmed.find(". ") {
            if pos <= 3 { // "1. " 到 "99. "
                let task_desc = trimmed[pos + 2..].to_string();
                
                // 解析任务类型
                let task_type = if task_desc.contains("(类型: 创建)") || 
                                   task_desc.contains("创建") {
                    TaskType::Coding
                } else if task_desc.contains("(类型: 修改)") || 
                          task_desc.contains("修改") {
                    TaskType::Coding
                } else if task_desc.contains("(类型: 分析)") ||
                          task_desc.contains("分析") {
                    TaskType::Analysis
                } else if task_desc.contains("(类型: 审查)") ||
                          task_desc.contains("审查") {
                    TaskType::Review
                } else {
                    TaskType::Simple
                };

                tasks.push(Task {
                    id: generate_task_id(),
                    description: task_desc,
                    task_type,
                    subtasks: vec![],
                    status: TaskStatus::Pending,
                    priority: 5,
                    dependencies: vec![],
                });
            }
        }
    }
    
    // 如果没有解析到子任务，创建一个默认任务
    if tasks.is_empty() {
        debug!("未能解析出子任务，创建默认任务");
        tasks.push(Task {
            id: generate_task_id(),
            description: "执行任务".to_string(),
            task_type: TaskType::Simple,
            subtasks: vec![],
            status: TaskStatus::Pending,
            priority: 5,
            dependencies: vec![],
        });
    }
    
    tasks
}
