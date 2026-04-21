//! Git tool implementations
//!
//! Provides git_status, git_diff, git_log, git_commit tools

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
async fn run_git(args: &[&str], workdir: &std::path::Path) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(workdir);

    let output = cmd.output().await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(anyhow::anyhow!("Git command failed: {}", stderr))
    }
}

/// 检查是否在 git 仓库中（异步版本）
async fn is_git_repo(workdir: &std::path::Path) -> bool {
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
            description: "View Git repository status, showing working directory and staging area file status. Returns current branch, modified files, untracked files, etc.",
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
                    "description": "Working directory path, defaults to current directory"
                },
                "short": {
                    "type": "boolean",
                    "description": "Use short format output, default false"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: GitStatusArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
        }

        let args = if params.short {
            vec!["status", "--short"]
        } else {
            vec!["status"]
        };

        let status = run_git(&args, actual_workdir).await?;

        // 获取当前分支
        let branch = run_git(&["branch", "--show-current"], actual_workdir).await?;

        Ok(format!("Branch: {}\n\n{}", branch.trim(), status.trim()))
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
            description: "View Git diff, showing specific file changes. Can view unstaged changes, staged changes, or diff between two commits.",
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
                    "description": "Working directory path, defaults to current directory"
                },
                "file": {
                    "type": "string",
                    "description": "File path to view diff for (optional)"
                },
                "staged": {
                    "type": "boolean",
                    "description": "Show staged changes, default false"
                },
                "commit": {
                    "type": "string",
                    "description": "Commit hash comparison, e.g. 'HEAD~1' or 'main..feature'"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: GitDiffArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
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

        let diff = run_git(&git_args, actual_workdir).await?;

        if diff.trim().is_empty() {
            Ok("No diff".to_string())
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
                "View Git commit history log. Shows recent commits including hash, author, date, and message.",
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
                    "description": "Working directory path, defaults to current directory"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of commits to show, default 10"
                },
                "file": {
                    "type": "string",
                    "description": "Only show commits for this file (optional)"
                },
                "oneline": {
                    "type": "boolean",
                    "description": "Use one-line format, default true"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: GitLogArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
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

        let log = run_git(&git_args, actual_workdir).await?;

        if log.trim().is_empty() {
            Ok("No commits yet".to_string())
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
                "Create a Git commit. Optionally add all changed files first. Requires user confirmation.",
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
                    "description": "Commit message"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory path, defaults to current directory"
                },
                "add_all": {
                    "type": "boolean",
                    "description": "Whether to run git add . first, default true"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        let params: GitCommitArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
        }

        // 检查是否有更改需要提交
        let status = run_git(&["status", "--porcelain"], actual_workdir).await?;

        if status.trim().is_empty() {
            return Ok("No changes to commit".to_string());
        }

        // 如果需要，先添加所有更改
        if params.add_all {
            run_git(&["add", "."], actual_workdir).await?;
        }

        // 创建提交
        let commit_output = run_git(&["commit", "-m", &params.message], actual_workdir).await?;

        // 获取新提交的信息
        let last_commit = run_git(&["log", "-1", "--oneline"], actual_workdir).await?;

        Ok(format!(
            "Committed!\n{}\nNew commit: {}",
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
            description: "Add files to Git staging area. Can add specific files or all changed files.",
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
                    "description": "File paths to add. Use \".\" to add all changes"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory path, defaults to current directory"
                }
            },
            "required": ["files"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        #[derive(Debug, Deserialize)]
        struct GitAddArgs {
            files: Vec<String>,
            workdir: Option<String>,
        }

        let params: GitAddArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
        }

        let mut all_output = String::new();

        for file in &params.files {
            let output = run_git(&["add", file], actual_workdir).await?;
            all_output.push_str(&format!("Added: {}\n", file));
            if !output.trim().is_empty() {
                all_output.push_str(&output);
                all_output.push('\n');
            }
        }

        // 显示暂存区状态
        let status = run_git(&["status", "--short"], actual_workdir).await?;
        all_output.push_str("\nStaged files:\n");
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
            description: "Manage Git branches. Can list, create, or delete branches.",
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
                    "description": "Action type: list=all branches, create=new branch, delete=delete branch, current=show current branch"
                },
                "name": {
                    "type": "string",
                    "description": "Branch name (required for create/delete)"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory path, defaults to current directory"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value, workdir: &std::path::Path) -> anyhow::Result<String> {
        #[derive(Debug, Deserialize)]
        struct GitBranchArgs {
            action: String,
            name: Option<String>,
            workdir: Option<String>,
        }

        let params: GitBranchArgs = serde_json::from_value(args)?;

        // 使用参数中的workdir或默认的workdir
        let path_buf = if let Some(wd) = &params.workdir {
            if std::path::Path::new(wd).is_absolute() {
                std::path::PathBuf::from(wd)
            } else {
                workdir.join(wd)
            }
        } else {
            workdir.to_path_buf()
        };
        let actual_workdir = &path_buf;

        if !is_git_repo(actual_workdir).await {
            return Err(anyhow::anyhow!("Not a git repository"));
        }

        match params.action.as_str() {
            "list" => {
                let branches = run_git(&["branch", "-a"], actual_workdir).await?;
                Ok(branches)
            }
            "current" => {
                let branch = run_git(&["branch", "--show-current"], actual_workdir).await?;
                Ok(format!("Current branch: {}", branch.trim()))
            }
            "create" => {
                let name = params
                    .name
                    .ok_or_else(|| anyhow::anyhow!("Branch name required for create"))?;
                run_git(&["branch", &name], actual_workdir).await?;
                Ok(format!("Created branch: {}", name))
            }
            "delete" => {
                let name = params
                    .name
                    .ok_or_else(|| anyhow::anyhow!("Branch name required for delete"))?;
                run_git(&["branch", "-d", &name], actual_workdir).await?;
                Ok(format!("Deleted branch: {}", name))
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", params.action)),
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
