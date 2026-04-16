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
    /// 审查器 - 负责代码审查、分析
    Reviewer,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Reviewer => write!(f, "审查器"),
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
    /// 任务状态
    pub status: TaskStatus,
    /// 错误信息（失败时记录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

impl Task {
    /// 创建新任务
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            status: TaskStatus::Pending,
            error: None,
        }
    }

    /// 开始任务
    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.status != TaskStatus::Pending {
            anyhow::bail!("任务 {} 状态为 {:?}，无法启动", self.id, self.status);
        }
        self.status = TaskStatus::InProgress;
        Ok(())
    }

    /// 完成任务
    pub fn complete(&mut self) -> anyhow::Result<()> {
        if self.status != TaskStatus::InProgress {
            anyhow::bail!("任务 {} 状态为 {:?}，无法完成", self.id, self.status);
        }
        self.status = TaskStatus::Completed;
        Ok(())
    }

    /// 任务失败
    pub fn fail(&mut self, error: impl Into<String>) -> anyhow::Result<()> {
        self.status = TaskStatus::Failed;
        self.error = Some(error.into());
        Ok(())
    }

    /// 跳过任务
    pub fn skip(&mut self, reason: impl Into<String>) {
        self.status = TaskStatus::Skipped;
        self.error = Some(reason.into());
    }

    /// 是否已完成（成功或失败）
    pub fn is_finished(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped)
    }
}


/// Agent 执行响应
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// 输出文本
    pub output: String,
    /// 是否成功
    pub success: bool,
    /// 需要的工具调用
    pub tool_calls: Vec<ToolCallRequest>,
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
            current_task: None,
            tool_results: Vec::new(),
        }
    }
    
    /// 添加工具结果
    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.tool_results.push(result);
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