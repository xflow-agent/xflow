//! xflow 工具系统
//!
//! 提供工具 trait 定义和内置工具实现

mod agent_tool;
mod edit_file;
mod git;
mod list_directory;
mod read_file;
mod run_shell;
mod search_file;
mod tool;
mod write_file;

pub use agent_tool::ReviewerAgentTool;
pub use edit_file::EditFileTool;
pub use git::{GitAddTool, GitBranchTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};
pub use list_directory::ListDirectoryTool;
pub use read_file::ReadFileTool;
pub use run_shell::{analyze_command, DangerAnalysis, RunShellTool};
pub use search_file::SearchFileTool;
pub use tool::{
    ResultDisplayType, Tool, ToolCall, ToolCategory, ToolDefinition, ToolDisplayConfig,
    ToolMetadata,
};
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

/// 创建默认工具注册表
pub fn create_default_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // 文件工具
    registry.register(Arc::new(ReadFileTool::new()));
    registry.register(Arc::new(WriteFileTool::new()));
    registry.register(Arc::new(EditFileTool::new())); // 新增 edit_file
    registry.register(Arc::new(ListDirectoryTool::new()));
    registry.register(Arc::new(SearchFileTool::new()));

    // Shell 工具
    registry.register(Arc::new(RunShellTool::new()));

    // Git 工具
    registry.register(Arc::new(GitStatusTool::new()));
    registry.register(Arc::new(GitDiffTool::new()));
    registry.register(Arc::new(GitLogTool::new()));
    registry.register(Arc::new(GitCommitTool::new()));
    registry.register(Arc::new(GitAddTool::new()));
    registry.register(Arc::new(GitBranchTool::new()));

    // Agent 工具（高级工具）
    registry.register(Arc::new(ReviewerAgentTool::new()));
    registry
}
