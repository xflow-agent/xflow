//! Agent 核心抽象
//!
//! 定义所有 Agent 的通用接口和共享类型

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xflow_model::ModelProvider;

/// Agent 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentType {
    /// 规划器 - 负责任务分解
    Planner,
    /// 编码器 - 负责代码编写
    Coder,
    /// 审查器 - 负责代码审查
    Reviewer,
    /// 通用 Agent - 处理一般任务
    General,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Planner => write!(f, "规划器"),
            AgentType::Coder => write!(f, "编码器"),
            AgentType::Reviewer => write!(f, "审查器"),
            AgentType::General => write!(f, "通用"),
        }
    }
}

/// 任务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务 ID
    pub id: String,
    /// 任务描述
    pub description: String,
    /// 任务类型
    pub task_type: TaskType,
    /// 子任务列表
    pub subtasks: Vec<Task>,
    /// 任务状态
    pub status: TaskStatus,
    /// 优先级 (1-10, 10 最高)
    pub priority: u8,
    /// 依赖的任务 ID
    pub dependencies: Vec<String>,
}

/// 任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// 复杂任务 - 需要分解
    Complex,
    /// 编码任务
    Coding,
    /// 审查任务
    Review,
    /// 分析任务
    Analysis,
    /// 简单任务 - 直接执行
    Simple,
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 待处理
    Pending,
    /// 进行中
    InProgress,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已跳过
    Skipped,
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

/// Agent 执行响应
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// 输出文本
    pub output: String,
    /// 是否成功
    pub success: bool,
    /// 生成的子任务
    pub subtasks: Vec<Task>,
    /// 需要的工具调用
    pub tool_calls: Vec<ToolCallRequest>,
    /// 下一步建议
    pub next_steps: Vec<String>,
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// 工具名称
    pub name: String,
    /// 工具参数
    pub arguments: serde_json::Value,
}

/// Agent 共享上下文
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// 工作目录
    pub workdir: std::path::PathBuf,
    /// 项目语言
    pub language: Option<String>,
    /// 相关文件
    pub relevant_files: Vec<String>,
    /// 之前任务的结果
    pub previous_results: Vec<AgentResponse>,
    /// 当前任务
    pub current_task: Option<Task>,
}

impl AgentContext {
    /// 创建新的 Agent 上下文
    pub fn new(workdir: std::path::PathBuf) -> Self {
        Self {
            workdir,
            language: None,
            relevant_files: Vec::new(),
            previous_results: Vec::new(),
            current_task: None,
        }
    }

    /// 添加相关文件
    pub fn add_file(&mut self, file: String) {
        if !self.relevant_files.contains(&file) {
            self.relevant_files.push(file);
        }
    }

    /// 添加之前的结果
    pub fn add_result(&mut self, result: AgentResponse) {
        self.previous_results.push(result);
    }
}

/// Agent trait - 所有 Agent 的基础接口
#[async_trait]
pub trait Agent: Send + Sync {
    /// 获取 Agent 类型
    fn agent_type(&self) -> AgentType;

    /// 获取 Agent 名称
    fn name(&self) -> &str;

    /// 获取 Agent 描述
    fn description(&self) -> &str;

    /// 判断是否能处理该任务
    fn can_handle(&self, task: &Task) -> bool;

    /// 执行任务
    async fn execute(
        &self,
        task: &Task,
        context: &AgentContext,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<AgentResponse>;

    /// 获取系统提示词
    fn system_prompt(&self) -> String;
}

/// 根据任务类型选择合适的 Agent
pub fn select_agent_for_task(task_type: TaskType) -> AgentType {
    match task_type {
        TaskType::Complex => AgentType::Planner,
        TaskType::Coding => AgentType::Coder,
        TaskType::Review => AgentType::Reviewer,
        TaskType::Analysis => AgentType::Reviewer,
        TaskType::Simple => AgentType::General,
    }
}

/// 创建任务 ID
pub fn generate_task_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("task-{}", duration.as_millis())
}
