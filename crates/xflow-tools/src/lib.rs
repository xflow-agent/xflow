//! xflow 工具系统
//!
//! 提供工具 trait 定义和内置工具实现

mod agent_executor;
mod agent_tool;
mod edit_file;
mod git;
mod list_directory;
mod read_file;
mod run_shell;
mod search_file;
mod tool;
mod write_file;

pub use agent_executor::AgentExecutor;
pub use agent_tool::ReviewerAgentTool;
pub use edit_file::EditFileTool;
pub use git::{GitAddTool, GitBranchTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};
pub use list_directory::ListDirectoryTool;
pub use read_file::ReadFileTool;
pub use run_shell::{analyze_command, DangerAnalysis, RunShellTool};
pub use search_file::SearchFileTool;
pub use tool::{
    ResultDisplayType, Tool, ToolCategory, ToolConfirmationRequest, ToolDefinition,
    ToolDisplayConfig, ToolMetadata,
};

// 为了向后兼容，保留别名（已废弃）
#[deprecated(note = "使用 Tool::build_confirmation 返回值判断，不再需要此字段")]
pub use tool::ToolMetadata as _ToolMetadataDeprecated;
pub use write_file::WriteFileTool;

use std::collections::HashMap;
use std::sync::Arc;

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// 获取工具
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// 获取所有工具定义（用于告诉模型可用工具）
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建默认工具注册表（不含 Agent 工具）
pub fn create_default_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    registry.register(Arc::new(ReadFileTool::new()));
    registry.register(Arc::new(WriteFileTool::new()));
    registry.register(Arc::new(EditFileTool::new()));
    registry.register(Arc::new(ListDirectoryTool::new()));
    registry.register(Arc::new(SearchFileTool::new()));
    registry.register(Arc::new(RunShellTool::new()));

    registry.register(Arc::new(GitStatusTool::new()));
    registry.register(Arc::new(GitDiffTool::new()));
    registry.register(Arc::new(GitLogTool::new()));
    registry.register(Arc::new(GitCommitTool::new()));
    registry.register(Arc::new(GitAddTool::new()));
    registry.register(Arc::new(GitBranchTool::new()));

    registry
}

/// 创建带 Agent 执行器的完整工具注册表
pub fn create_default_tools_with_agent(executor: Arc<dyn AgentExecutor>) -> ToolRegistry {
    let mut registry = create_default_tools();
    registry.register(Arc::new(ReviewerAgentTool::with_executor(executor)));
    registry
}
