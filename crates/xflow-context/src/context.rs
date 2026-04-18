//! 上下文构建器
//!
//! 构建注入到系统提示词中的项目上下文信息

use crate::scanner::{ProjectInfo, ProjectScanner};
use crate::token_estimator::TokenEstimator;
use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

/// 项目上下文
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// 项目信息
    pub info: ProjectInfo,
    /// 上下文文本（用于注入到系统提示词）
    pub context_text: String,
    /// Token 数量估算
    pub token_count: usize,
}

/// 上下文构建器
pub struct ContextBuilder {
    /// 项目扫描器
    scanner: ProjectScanner,
    /// Token 估算器
    estimator: TokenEstimator,
    /// 最大上下文 Token 数
    max_tokens: usize,
    /// 是否包含目录树
    include_tree: bool,
    /// 是否包含文件统计
    include_stats: bool,
}

impl ContextBuilder {
    /// 创建新的上下文构建器
    pub fn new(root: PathBuf) -> Self {
        Self {
            scanner: ProjectScanner::new(root),
            estimator: TokenEstimator::new(),
            max_tokens: 2000, // 默认最多 2000 tokens 用于上下文
            include_tree: true,
            include_stats: true,
        }
    }

    /// 设置最大 Token 数
    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }

    /// 设置是否包含目录树
    pub fn with_tree(mut self, include: bool) -> Self {
        self.include_tree = include;
        self
    }

    /// 设置是否包含文件统计
    pub fn with_stats(mut self, include: bool) -> Self {
        self.include_stats = include;
        self
    }

    /// 构建项目上下文
    pub fn build(&self) -> Result<ProjectContext> {
        info!("构建项目上下文...");

        // 扫描项目
        let info = self.scanner.scan()?;

        // 构建上下文文本
        let context_text = self.build_context_text(&info);

        // 估算 Token 数
        let token_count = self.estimator.estimate(&context_text);

        info!("项目上下文构建完成: {} tokens", token_count);

        Ok(ProjectContext {
            info,
            context_text,
            token_count,
        })
    }

    /// 构建上下文文本
    fn build_context_text(&self, info: &ProjectInfo) -> String {
        let mut parts = Vec::new();

        // 项目概览
        parts.push(format!(
            "## 项目信息\n\n\
            - 名称: {}\n\
            - 类型: {}\n\
            - 根目录: {}\n",
            info.name,
            info.project_type.display_name(),
            info.root.display()
        ));

        // 语言统计
        if self.include_stats && !info.languages.is_empty() {
            let lang_stats: Vec<String> = info
                .languages
                .iter()
                .take(5)
                .map(|lang| {
                    let count = info.language_stats.get(lang).unwrap_or(&0);
                    format!("{} ({} 文件)", lang.display_name(), count)
                })
                .collect();

            parts.push(format!("\n## 主要语言\n\n{}\n", lang_stats.join(", ")));
        }

        // 目录结构（简化）
        if self.include_tree {
            let tree = self.build_directory_tree(info);
            if !tree.is_empty() {
                parts.push(format!("\n## 目录结构\n\n```\n{}\n```\n", tree));
            }
        }

        // 源文件列表（按语言分组，只显示重要的）
        let important_files = self.get_important_files(info);
        if !important_files.is_empty() {
            parts.push(format!("\n## 重要文件\n\n{}\n", important_files.join("\n")));
        }

        parts.join("")
    }

    /// 构建简化的目录树
    fn build_directory_tree(&self, info: &ProjectInfo) -> String {
        let mut lines = Vec::new();
        lines.push(info.name.clone());

        // 按目录分组
        let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for file in &info.source_files {
            if let Some(parent) = file.path.parent() {
                if !parent.as_os_str().is_empty() {
                    // 只取第一级目录
                    if let Some(first) = parent.iter().next() {
                        dirs.insert(first.to_string_lossy().to_string());
                    }
                }
            }
        }

        // 显示目录
        let mut dirs: Vec<String> = dirs.into_iter().collect();
        dirs.sort();

        for (i, dir) in dirs.iter().enumerate() {
            let prefix = if i == dirs.len() - 1 {
                "└── "
            } else {
                "├── "
            };
            lines.push(format!("{}{}/", prefix, dir));
        }

        // 限制行数
        if lines.len() > 20 {
            lines.truncate(20);
            lines.push("... (更多目录)".to_string());
        }

        lines.join("\n")
    }

    /// 获取重要文件列表
    fn get_important_files(&self, info: &ProjectInfo) -> Vec<String> {
        let important_names = [
            "Cargo.toml",
            "package.json",
            "go.mod",
            "pyproject.toml",
            "pom.xml",
            "build.gradle",
            "Makefile",
            "Dockerfile",
            "README.md",
            "LICENSE",
            ".gitignore",
            "main.rs",
            "lib.rs",
            "main.go",
            "main.py",
            "main.java",
            "index.ts",
            "index.js",
            "app.ts",
            "app.js",
        ];

        let mut result = Vec::new();

        for file in &info.files {
            let filename = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if important_names.contains(&filename) {
                result.push(format!(
                    "- `{}` ({})",
                    file.path.display(),
                    file.language.display_name()
                ));
            }
        }

        // 限制数量
        if result.len() > 15 {
            result.truncate(15);
            result.push("- ... (更多文件)".to_string());
        }

        result
    }

    /// 生成注入到系统提示词的上下文
    pub fn generate_system_context(&self) -> Result<String> {
        let context = self.build()?;

        Ok(format!(
            "\n---\n\n# 当前项目上下文\n\n{}\n\n**提示**: 当用户提到项目文件或代码时，请优先参考上述项目结构。如果需要查看具体文件内容，请使用 `read_file` 工具。",
            context.context_text
        ))
    }
}

impl ProjectContext {
    /// 获取项目摘要（简短版）
    pub fn brief_summary(&self) -> String {
        format!(
            "项目: {} ({}) - {} 源文件",
            self.info.name,
            self.info.project_type.display_name(),
            self.info.source_files_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project() -> TempDir {
        let dir = tempfile::tempdir().unwrap();

        // 创建 Rust 项目结构
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        dir
    }

    #[test]
    fn test_build_context() {
        let dir = create_test_project();
        let builder = ContextBuilder::new(dir.path().to_path_buf());
        let context = builder.build().unwrap();

        assert!(context.context_text.contains("test"));
        assert!(context.token_count > 0);
    }

    #[test]
    fn test_generate_system_context() {
        let dir = create_test_project();
        let builder = ContextBuilder::new(dir.path().to_path_buf());
        let system_context = builder.generate_system_context().unwrap();

        assert!(system_context.contains("项目信息"));
        assert!(system_context.contains("read_file"));
    }
}
