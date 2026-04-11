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
#[derive(Default)]
pub enum TaskStatus {
    /// 待处理
    #[default]
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

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// 工具名称
    pub name: String,
    /// 执行结果
    pub result: String,
    /// 是否成功
    pub success: bool,
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
    /// 工具执行结果（本轮）
    pub tool_results: Vec<ToolResult>,
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
            tool_results: Vec::new(),
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
    
    /// 添加工具结果
    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.tool_results.push(result);
    }
    
    /// 清空工具结果
    pub fn clear_tool_results(&mut self) {
        self.tool_results.clear();
    }
    
    /// 获取工具结果的文本描述（用于注入到 prompt）
    pub fn tool_results_summary(&self) -> String {
        if self.tool_results.is_empty() {
            return String::new();
        }
        
        let mut summary = String::from("已执行的工具及其结果:\n");
        for (i, tr) in self.tool_results.iter().enumerate() {
            // 安全截断，避免 UTF-8 边界问题
            let truncated = if tr.result.len() > 3000 {
                let safe_end = tr.result.char_indices()
                    .take_while(|(idx, _)| *idx < 3000)
                    .last()
                    .map(|(idx, c)| idx + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...(共 {} 字节)", &tr.result[..safe_end], tr.result.len())
            } else {
                tr.result.clone()
            };
            summary.push_str(&format!(
                "\n{}. 工具: {}\n   状态: {}\n   结果:\n{}\n",
                i + 1,
                tr.name,
                if tr.success { "成功" } else { "失败" },
                truncated
            ));
        }
        summary
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
