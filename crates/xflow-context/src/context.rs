use crate::scanner::{ProjectInfo, ProjectScanner};
use crate::token_estimator::TokenEstimator;
use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub info: ProjectInfo,
    pub context_text: String,
    pub token_count: usize,
}

pub struct ContextBuilder {
    scanner: ProjectScanner,
    estimator: TokenEstimator,
    max_tokens: usize,
    include_tree: bool,
    include_stats: bool,
}

impl ContextBuilder {
    pub fn new(root: PathBuf) -> Self {
        Self {
            scanner: ProjectScanner::new(root),
            estimator: TokenEstimator::new(),
            max_tokens: 2000,
            include_tree: true,
            include_stats: true,
        }
    }

    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }

    pub fn with_tree(mut self, include: bool) -> Self {
        self.include_tree = include;
        self
    }

    pub fn with_stats(mut self, include: bool) -> Self {
        self.include_stats = include;
        self
    }

    pub fn build(&self) -> Result<ProjectContext> {
        info!("Building project context...");

        let info = self.scanner.scan()?;
        let context_text = self.build_context_text(&info);
        let token_count = self.estimator.estimate(&context_text);

        info!("Project context built: {} tokens", token_count);

        Ok(ProjectContext {
            info,
            context_text,
            token_count,
        })
    }

    fn build_context_text(&self, info: &ProjectInfo) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "## Project Info\n\n\
            - Name: {}\n\
            - Type: {}\n\
            - Root: {}\n",
            info.name,
            info.project_type.display_name(),
            info.root.display()
        ));

        if self.include_stats && !info.languages.is_empty() {
            let lang_stats: Vec<String> = info
                .languages
                .iter()
                .take(5)
                .map(|lang| {
                    let count = info.language_stats.get(lang).unwrap_or(&0);
                    format!("{} ({} files)", lang.display_name(), count)
                })
                .collect();

            parts.push(format!("\n## Languages\n\n{}\n", lang_stats.join(", ")));
        }

        if self.include_tree {
            let tree = self.build_directory_tree(info);
            if !tree.is_empty() {
                parts.push(format!("\n## Directory Structure\n\n```\n{}\n```\n", tree));
            }
        }

        let important_files = self.get_important_files(info);
        if !important_files.is_empty() {
            parts.push(format!("\n## Key Files\n\n{}\n", important_files.join("\n")));
        }

        parts.join("")
    }

    fn build_directory_tree(&self, info: &ProjectInfo) -> String {
        let mut lines = Vec::new();
        lines.push(info.name.clone());

        let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for file in &info.source_files {
            if let Some(parent) = file.path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Some(first) = parent.iter().next() {
                        dirs.insert(first.to_string_lossy().to_string());
                    }
                }
            }
        }

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

        if lines.len() > 20 {
            lines.truncate(20);
            lines.push("... (more directories)".to_string());
        }

        lines.join("\n")
    }

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

        if result.len() > 15 {
            result.truncate(15);
            result.push("- ... (more files)".to_string());
        }

        result
    }

    pub fn generate_system_context(&self) -> Result<String> {
        let context = self.build()?;

        Ok(format!(
            "\n---\n\n# Current Project Context\n\n{}\n\n**Note**: When the user mentions project files or code, refer to the project structure above. Use the `read_file` tool to view specific file contents.",
            context.context_text
        ))
    }
}

impl ProjectContext {
    pub fn brief_summary(&self) -> String {
        format!(
            "Project: {} ({}) - {} source files",
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

        assert!(!context.context_text.is_empty());
        assert!(context.token_count > 0);
    }

    #[test]
    fn test_generate_system_context() {
        let dir = create_test_project();
        let builder = ContextBuilder::new(dir.path().to_path_buf());
        let system_context = builder.generate_system_context().unwrap();

        assert!(system_context.contains("Project Info"));
        assert!(system_context.contains("read_file"));
    }
}
