//! Agent 协调器
//!
//! 负责协调多个 Agent 之间的协作

use crate::agent::{
    Agent, AgentContext, AgentResponse, AgentType, Task, TaskType, TaskStatus,
    generate_task_id, select_agent_for_task,
};
use crate::{PlannerAgent, CoderAgent, ReviewerAgent};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_model::ModelProvider;
use xflow_tools::ToolRegistry;

/// Agent 协调器
pub struct AgentCoordinator {
    /// 可用的 Agent 列表
    agents: HashMap<AgentType, Arc<dyn Agent>>,
    /// 模型提供者
    provider: Arc<dyn ModelProvider>,
    /// 工具注册表
    tools: ToolRegistry,
    /// 工作目录
    workdir: PathBuf,
    /// 最大任务执行次数
    max_iterations: usize,
}

impl AgentCoordinator {
    /// 创建新的协调器
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf) -> Self {
        let mut agents: HashMap<AgentType, Arc<dyn Agent>> = HashMap::new();
        
        // 注册默认 Agent
        agents.insert(AgentType::Planner, Arc::new(PlannerAgent::new()));
        agents.insert(AgentType::Coder, Arc::new(CoderAgent::new()));
        agents.insert(AgentType::Reviewer, Arc::new(ReviewerAgent::new()));
        
        Self {
            agents,
            provider,
            tools: xflow_tools::create_default_tools(),
            workdir,
            max_iterations: 20,
        }
    }

    /// 注册自定义 Agent
    pub fn register_agent(&mut self, agent: Arc<dyn Agent>) {
        self.agents.insert(agent.agent_type(), agent);
    }

    /// 设置最大迭代次数
    pub fn set_max_iterations(&mut self, max: usize) {
        self.max_iterations = max;
    }

    /// 处理用户请求（主入口）
    pub async fn process(&self, request: &str) -> Result<CoordinatorResult> {
        info!("协调器开始处理请求: {}", request);
        
        // 创建初始任务
        let task = Task {
            id: generate_task_id(),
            description: request.to_string(),
            task_type: TaskType::Complex, // 初始假设为复杂任务
            subtasks: vec![],
            status: TaskStatus::Pending,
            priority: 5,
            dependencies: vec![],
        };
        
        // 创建上下文
        let context = AgentContext::new(self.workdir.clone());
        
        // 分析并执行任务
        self.execute_task(&task, context).await
    }

    /// 执行任务
    async fn execute_task(
        &self,
        task: &Task,
        mut context: AgentContext,
    ) -> Result<CoordinatorResult> {
        let mut results = Vec::new();
        let mut current_tasks = vec![task.clone()];
        let mut iteration = 0;
        
        while !current_tasks.is_empty() && iteration < self.max_iterations {
            iteration += 1;
            info!("执行迭代 {}, 剩余 {} 个任务", iteration, current_tasks.len());
            
            let mut next_tasks = Vec::new();
            
            for task in current_tasks {
                if task.status == TaskStatus::Completed {
                    continue;
                }
                
                // 选择合适的 Agent
                let agent_type = select_agent_for_task(task.task_type);
                
                match self.execute_with_agent(&task, &context, agent_type).await {
                    Ok(response) => {
                        // 更新上下文
                        context.add_result(response.clone());
                        
                        // 添加子任务
                        for subtask in &response.subtasks {
                            next_tasks.push(subtask.clone());
                        }
                        
                        // 执行工具调用
                        for tool_call in &response.tool_calls {
                            if let Some(tool) = self.tools.get(&tool_call.name) {
                                println!("\n[执行工具: {}]", tool_call.name);
                                match tool.execute(tool_call.arguments.clone()).await {
                                    Ok(result) => {
                                        println!("[结果: {} 字节]", result.len());
                                    }
                                    Err(e) => {
                                        warn!("工具执行失败: {}", e);
                                    }
                                }
                            }
                        }
                        
                        results.push(TaskResult {
                            task_id: task.id.clone(),
                            agent_type,
                            output: response.output.clone(),
                            success: response.success,
                        });
                    }
                    Err(e) => {
                        warn!("任务执行失败: {}", e);
                        results.push(TaskResult {
                            task_id: task.id.clone(),
                            agent_type,
                            output: format!("错误: {}", e),
                            success: false,
                        });
                    }
                }
            }
            
            current_tasks = next_tasks;
        }
        
        Ok(CoordinatorResult {
            success: results.iter().all(|r| r.success),
            task_results: results,
            iterations: iteration,
        })
    }

    /// 使用指定 Agent 执行任务
    async fn execute_with_agent(
        &self,
        task: &Task,
        context: &AgentContext,
        agent_type: AgentType,
    ) -> Result<AgentResponse> {
        let agent = self.agents.get(&agent_type)
            .ok_or_else(|| anyhow::anyhow!("未找到 {:?} Agent", agent_type))?;
        
        println!("\n{} [{}] 处理任务", 
            agent_type, agent.name());
        println!("任务: {}", task.description);
        println!("{}", "─".repeat(50));
        
        agent.execute(task, context, self.provider.clone()).await
    }

    /// 获取可用 Agent 列表
    pub fn available_agents(&self) -> Vec<AgentType> {
        self.agents.keys().copied().collect()
    }

    /// 分析任务类型
    pub fn analyze_task(&self, description: &str) -> TaskType {
        let lower = description.to_lowercase();
        
        // 简单的关键词匹配
        if lower.contains("创建") || lower.contains("实现") || lower.contains("编写") {
            if lower.contains("并") || lower.contains("然后") || lower.contains("之后") {
                return TaskType::Complex;
            }
            return TaskType::Coding;
        }
        
        if lower.contains("修改") || lower.contains("修复") || lower.contains("重构") {
            return TaskType::Coding;
        }
        
        if lower.contains("审查") || lower.contains("检查") || lower.contains("分析") {
            return TaskType::Analysis;
        }
        
        if lower.contains("review") || lower.contains("检查代码质量") {
            return TaskType::Review;
        }
        
        TaskType::Simple
    }
}

/// 协调器执行结果
#[derive(Debug)]
pub struct CoordinatorResult {
    /// 是否全部成功
    pub success: bool,
    /// 各任务结果
    pub task_results: Vec<TaskResult>,
    /// 总迭代次数
    pub iterations: usize,
}

/// 单个任务结果
#[derive(Debug)]
pub struct TaskResult {
    /// 任务 ID
    pub task_id: String,
    /// 执行的 Agent 类型
    pub agent_type: AgentType,
    /// 输出内容
    pub output: String,
    /// 是否成功
    pub success: bool,
}

impl CoordinatorResult {
    /// 打印摘要
    pub fn print_summary(&self) {
        println!("\n{}", "═".repeat(50));
        println!("执行摘要");
        println!("{}", "═".repeat(50));
        
        for (i, result) in self.task_results.iter().enumerate() {
            let status = if result.success { "✓" } else { "✗" };
            println!("{}. {} [{}] {}", i + 1, status, result.agent_type, 
                result.output.chars().take(50).collect::<String>());
        }
        
        println!("{}", "─".repeat(50));
        println!("总计: {} 个任务, {} 轮迭代", 
            self.task_results.len(), self.iterations);
        println!("状态: {}", if self.success { "成功" } else { "部分失败" });
    }
}
