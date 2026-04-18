//! Git 工具实现
//!
//! 提供 git_status, git_diff, git_log, git_commit 工具

use crate::tool::{ResultDisplayType, Tool, ToolCategory, ToolDisplayConfig, ToolMetadata};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;

/// git_status 参数
#[derive(Debug, Deserialize)]
struct GitStatusArgs {
    /// 工作目录
    #[serde(default)]
    workdir: Option<String>,
    /// 是否显示简短格式
    #[serde(default)]
    short: bool,
}

/// git_diff 参数
#[derive(Debug, Deserialize)]
struct GitDiffArgs {
    /// 工作目录
    #[serde(default)]
    workdir: Option<String>,
    /// 比较的文件路径（可选）
    #[serde(default)]
    file: Option<String>,
    /// 是否显示暂存区的更改
    #[serde(default)]
    staged: bool,
    /// 提交哈希比较（如 "HEAD~1" 或 "main..feature"）
    #[serde(default)]
    commit: Option<String>,
}

/// git_log 参数
#[derive(Debug, Deserialize)]
struct GitLogArgs {
    /// 工作目录
    #[serde(default)]
    workdir: Option<String>,
    /// 显示的提交数量
    #[serde(default = "default_count")]
    count: usize,
    /// 文件路径（可选，只显示该文件的提交）
    #[serde(default)]
    file: Option<String>,
    /// 是否显示单行格式
    #[serde(default = "default_true")]
    oneline: bool,
}

fn default_count() -> usize {
    10
}

fn default_true() -> bool {
    true
}

/// git_commit 参数
#[derive(Debug, Deserialize)]
struct GitCommitArgs {
    /// 提交消息
    message: String,
    /// 工作目录
    #[serde(default)]
    workdir: Option<String>,
    /// 是否添加所有更改的文件
    #[serde(default = "default_true")]
    add_all: bool,
}

/// 执行 git 命令的辅助函数（异步版本）
async fn run_git(args: &[&str], workdir: Option<&str>) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }

    let output = cmd.output().await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(anyhow::anyhow!("Git 命令失败: {}", stderr))
    }
}

/// 检查是否在 git 仓库中（异步版本）
async fn is_git_repo(workdir: Option<&str>) -> bool {
    run_git(&["rev-parse", "--is-inside-work-tree"], workdir)
        .await
        .is_ok()
}

// ============================================================================
// GitStatusTool
// ============================================================================

/// Git 状态工具
pub struct GitStatusTool;

impl Default for GitStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitStatusTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_status",
            description: "查看 Git 仓库状态，显示工作目录和暂存区的文件状态。返回当前分支、已修改文件、未跟踪文件等信息。",
            category: ToolCategory::Git,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "",
                result_display: ResultDisplayType::Full,
                max_preview_lines: 20,
                max_preview_chars: 1000,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                },
                "short": {
                    "type": "boolean",
                    "description": "是否使用简短格式输出，默认 false"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: GitStatusArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        let args = if params.short {
            vec!["status", "--short"]
        } else {
            vec!["status"]
        };

        let status = run_git(&args, workdir).await?;

        // 获取当前分支
        let branch = run_git(&["branch", "--show-current"], workdir).await?;

        Ok(format!("当前分支: {}\n\n{}", branch.trim(), status.trim()))
    }
}

// ============================================================================
// GitDiffTool
// ============================================================================

/// Git 差异工具
pub struct GitDiffTool;

impl Default for GitDiffTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitDiffTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_diff",
            description: "查看 Git 差异，显示文件的具体更改内容。可以查看未暂存的更改、暂存区的更改，或两个提交之间的差异。",
            category: ToolCategory::Git,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "file",
                result_display: ResultDisplayType::Full,
                max_preview_lines: 50,
                max_preview_chars: 2000,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                },
                "file": {
                    "type": "string",
                    "description": "要查看差异的文件路径（可选）"
                },
                "staged": {
                    "type": "boolean",
                    "description": "是否显示暂存区的更改，默认 false"
                },
                "commit": {
                    "type": "string",
                    "description": "提交哈希比较，如 'HEAD~1' 或 'main..feature'"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: GitDiffArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        // 构建命令参数
        let mut git_args = vec!["diff"];

        if params.staged {
            git_args.push("--staged");
        }

        if let Some(commit) = &params.commit {
            git_args.push(commit);
        }

        if let Some(file) = &params.file {
            git_args.push("--");
            git_args.push(file);
        }

        let diff = run_git(&git_args, workdir).await?;

        if diff.trim().is_empty() {
            Ok("没有差异".to_string())
        } else {
            Ok(diff)
        }
    }
}

// ============================================================================
// GitLogTool
// ============================================================================

/// Git 日志工具
pub struct GitLogTool;

impl Default for GitLogTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLogTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitLogTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_log",
            description:
                "查看 Git 提交历史日志。显示最近的提交记录，包括提交哈希、作者、日期和提交消息。",
            category: ToolCategory::Git,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "file",
                result_display: ResultDisplayType::LineCount,
                max_preview_lines: 20,
                max_preview_chars: 1000,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                },
                "count": {
                    "type": "integer",
                    "description": "显示的提交数量，默认 10"
                },
                "file": {
                    "type": "string",
                    "description": "只显示该文件的提交历史（可选）"
                },
                "oneline": {
                    "type": "boolean",
                    "description": "是否使用单行格式，默认 true"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: GitLogArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        let mut git_args = vec!["log"];

        if params.oneline {
            git_args.push("--oneline");
        } else {
            // 详细格式
            git_args.push("--format=%h - %an, %ar : %s");
        }

        git_args.push("-n");
        let count_str = params.count.to_string();
        git_args.push(&count_str);

        if let Some(file) = &params.file {
            git_args.push("--");
            git_args.push(file);
        }

        let log = run_git(&git_args, workdir).await?;

        if log.trim().is_empty() {
            Ok("没有提交历史".to_string())
        } else {
            Ok(log)
        }
    }
}

// ============================================================================
// GitCommitTool
// ============================================================================

/// Git 提交工具
///
/// 注意：此工具需要用户确认后才能执行
pub struct GitCommitTool;

impl Default for GitCommitTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitCommitTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_commit",
            description:
                "创建 Git 提交。可以选择先添加所有更改的文件，然后创建提交。此操作需要用户确认。",
            category: ToolCategory::Git,
            danger_level: 1,
            display: ToolDisplayConfig {
                primary_param: "message",
                result_display: ResultDisplayType::Full,
                max_preview_lines: 10,
                max_preview_chars: 500,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "提交消息"
                },
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                },
                "add_all": {
                    "type": "boolean",
                    "description": "是否先执行 git add . 添加所有更改，默认 true"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: GitCommitArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        // 检查是否有更改需要提交
        let status = run_git(&["status", "--porcelain"], workdir).await?;

        if status.trim().is_empty() {
            return Ok("没有更改需要提交".to_string());
        }

        // 如果需要，先添加所有更改
        if params.add_all {
            run_git(&["add", "."], workdir).await?;
        }

        // 创建提交
        let commit_output = run_git(&["commit", "-m", &params.message], workdir).await?;

        // 获取新提交的信息
        let last_commit = run_git(&["log", "-1", "--oneline"], workdir).await?;

        Ok(format!(
            "提交成功！\n{}\n新提交: {}",
            commit_output.trim(),
            last_commit.trim()
        ))
    }
}

// ============================================================================
// GitAddTool
// ============================================================================

/// Git 添加工具
pub struct GitAddTool;

impl Default for GitAddTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitAddTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitAddTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_add",
            description: "将文件添加到 Git 暂存区。可以添加指定文件或所有更改的文件。",
            category: ToolCategory::Git,
            danger_level: 0,
            display: ToolDisplayConfig {
                primary_param: "files",
                result_display: ResultDisplayType::StatusOnly,
                max_preview_lines: 5,
                max_preview_chars: 200,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "要添加的文件路径列表。使用 \".\" 添加所有更改"
                },
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                }
            },
            "required": ["files"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        #[derive(Debug, Deserialize)]
        struct GitAddArgs {
            files: Vec<String>,
            workdir: Option<String>,
        }

        let params: GitAddArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        let mut all_output = String::new();

        for file in &params.files {
            let output = run_git(&["add", file], workdir).await?;
            all_output.push_str(&format!("已添加: {}\n", file));
            if !output.trim().is_empty() {
                all_output.push_str(&output);
                all_output.push('\n');
            }
        }

        // 显示暂存区状态
        let status = run_git(&["status", "--short"], workdir).await?;
        all_output.push_str("\n当前暂存区状态:\n");
        all_output.push_str(&status);

        Ok(all_output)
    }
}

// ============================================================================
// GitBranchTool
// ============================================================================

/// Git 分支工具
pub struct GitBranchTool;

impl Default for GitBranchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitBranchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitBranchTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "git_branch",
            description: "管理 Git 分支。可以列出、创建或删除分支。",
            category: ToolCategory::Git,
            danger_level: 1,
            display: ToolDisplayConfig {
                primary_param: "action",
                result_display: ResultDisplayType::Full,
                max_preview_lines: 20,
                max_preview_chars: 800,
            },
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "delete", "current"],
                    "description": "操作类型：list=列出所有分支，create=创建新分支，delete=删除分支，current=显示当前分支"
                },
                "name": {
                    "type": "string",
                    "description": "分支名称（create/delete 操作时需要）"
                },
                "workdir": {
                    "type": "string",
                    "description": "工作目录路径，默认为当前目录"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        #[derive(Debug, Deserialize)]
        struct GitBranchArgs {
            action: String,
            name: Option<String>,
            workdir: Option<String>,
        }

        let params: GitBranchArgs = serde_json::from_value(args)?;

        let workdir = params.workdir.as_deref();

        if !is_git_repo(workdir).await {
            return Err(anyhow::anyhow!("当前目录不是 Git 仓库"));
        }

        match params.action.as_str() {
            "list" => {
                let branches = run_git(&["branch", "-a"], workdir).await?;
                Ok(branches)
            }
            "current" => {
                let branch = run_git(&["branch", "--show-current"], workdir).await?;
                Ok(format!("当前分支: {}", branch.trim()))
            }
            "create" => {
                let name = params
                    .name
                    .ok_or_else(|| anyhow::anyhow!("创建分支需要提供分支名称"))?;
                run_git(&["branch", &name], workdir).await?;
                Ok(format!("已创建分支: {}", name))
            }
            "delete" => {
                let name = params
                    .name
                    .ok_or_else(|| anyhow::anyhow!("删除分支需要提供分支名称"))?;
                run_git(&["branch", "-d", &name], workdir).await?;
                Ok(format!("已删除分支: {}", name))
            }
            _ => Err(anyhow::anyhow!("未知操作: {}", params.action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_status_tool() {
        let tool = GitStatusTool::new();
        assert_eq!(tool.name(), "git_status");
    }

    #[tokio::test]
    async fn test_git_diff_tool() {
        let tool = GitDiffTool::new();
        assert_eq!(tool.name(), "git_diff");
    }

    #[tokio::test]
    async fn test_git_log_tool() {
        let tool = GitLogTool::new();
        assert_eq!(tool.name(), "git_log");
    }

    #[tokio::test]
    async fn test_git_commit_tool() {
        let tool = GitCommitTool::new();
        assert_eq!(tool.name(), "git_commit");
    }
}
