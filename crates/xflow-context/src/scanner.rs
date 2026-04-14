//! 项目结构扫描器
//!
//! 扫描项目目录结构，识别文件类型和项目信息

use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};
use crate::language::{detect_language, is_source_file, Language};

/// 文件信息
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// 文件路径（相对于项目根目录）
    pub path: PathBuf,
    /// 语言类型
    pub language: Language,
    /// 文件大小（字节）
    pub size: u64,
    /// 是否是源代码文件
    pub is_source: bool,
}

/// 项目信息
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    /// 项目根目录
    pub root: PathBuf,
    /// 项目名称（从目录名推断）
    pub name: String,
    /// 主要编程语言（按文件数排序）
    pub languages: Vec<Language>,
    /// 语言统计（语言 -> 文件数）
    pub language_stats: HashMap<Language, usize>,
    /// 所有文件
    pub files: Vec<FileInfo>,
    /// 源代码文件
    pub source_files: Vec<FileInfo>,
    /// 项目类型（如 Rust, Node, Python 等）
    pub project_type: ProjectType,
    /// 总文件数
    pub total_files: usize,
    /// 源代码文件数
    pub source_files_count: usize,
}

/// 项目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    Mixed,
    Unknown,
}

impl ProjectType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectType::Rust => "Rust",
            ProjectType::Node => "Node.js",
            ProjectType::Python => "Python",
            ProjectType::Go => "Go",
            ProjectType::Java => "Java",
            ProjectType::Mixed => "Mixed",
            ProjectType::Unknown => "Unknown",
        }
    }
}

/// 项目扫描器
pub struct ProjectScanner {
    /// 项目根目录
    root: PathBuf,
    /// 最大扫描文件数
    max_files: usize,
    /// 最大文件大小（字节）
    max_file_size: u64,
}

impl ProjectScanner {
    /// 创建新的扫描器
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            max_files: 1000,
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
    
    /// 设置最大文件数
    pub fn with_max_files(mut self, max: usize) -> Self {
        self.max_files = max;
        self
    }
    
    /// 设置最大文件大小
    pub fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }
    
    /// 扫描项目
    pub fn scan(&self) -> Result<ProjectInfo> {
        info!("扫描项目目录: {:?}", self.root);
        
        let mut files = Vec::new();
        let mut language_stats: HashMap<Language, usize> = HashMap::new();
        
        // 使用 ignore 库自动处理 .gitignore
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)          // 包含隐藏文件
            .git_ignore(true)       // 尊重 .gitignore
            .git_global(true)       // 尊重全局 gitignore
            .git_exclude(true)      // 尊重 .git/info/exclude
            .max_depth(Some(10))    // 限制深度
            .build();
        
        let mut count = 0;
        for entry in walker {
            if count >= self.max_files {
                debug!("达到最大文件数限制: {}", self.max_files);
                break;
            }
            
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    debug!("跳过无法访问的条目: {}", e);
                    continue;
                }
            };
            
            // 只处理文件
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            
            // 获取文件元数据
            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(e) => {
                    debug!("无法读取文件元数据: {:?} - {}", path, e);
                    continue;
                }
            };
            
            // 跳过过大文件
            if metadata.len() > self.max_file_size {
                debug!("跳过大文件: {:?} ({} bytes)", path, metadata.len());
                continue;
            }
            
            // 计算相对路径
            let relative_path = path.strip_prefix(&self.root).unwrap_or(path).to_path_buf();
            
            // 检测语言
            let language = detect_language(path);
            let is_source = is_source_file(path);
            
            // 更新语言统计
            *language_stats.entry(language).or_insert(0) += 1;
            
            let file_info = FileInfo {
                path: relative_path,
                language,
                size: metadata.len(),
                is_source,
            };
            
            files.push(file_info);
            count += 1;
        }
        
        // 分离源代码文件
        let source_files: Vec<FileInfo> = files.iter()
            .filter(|f| f.is_source)
            .cloned()
            .collect();
        
        // 排序语言统计
        let mut lang_vec: Vec<(_, _)> = language_stats.iter()
            .filter(|(lang, _)| lang.is_programming_language())
            .map(|(k, v)| (*v, *k))
            .collect();
        lang_vec.sort_by(|a, b| b.0.cmp(&a.0));
        let languages: Vec<Language> = lang_vec.iter().map(|(_, l)| *l).collect();
        
        // 检测项目类型
        let project_type = self.detect_project_type(&files);
        
        // 获取项目名称（规范化路径后获取）
        let name = self.root
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .or_else(|| self.root.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        
        let total_files = files.len();
        let source_files_count = source_files.len();
        
        info!(
            "扫描完成: {} 个文件, {} 个源文件, 主要语言: {:?}",
            total_files, source_files_count, languages.first()
        );
        
        Ok(ProjectInfo {
            root: self.root.clone(),
            name,
            languages,
            language_stats,
            files,
            source_files,
            project_type,
            total_files,
            source_files_count,
        })
    }
    
    /// 检测项目类型
    fn detect_project_type(&self, files: &[FileInfo]) -> ProjectType {
        use std::path::Path;
        let has_cargo_toml = files.iter().any(|f| f.path.as_path() == Path::new("Cargo.toml"));
        let has_package_json = files.iter().any(|f| f.path.as_path() == Path::new("package.json"));
        let has_pyproject = files.iter().any(|f| 
            f.path.as_path() == Path::new("pyproject.toml") ||
            f.path.as_path() == Path::new("setup.py") ||
            f.path.as_path() == Path::new("requirements.txt")
        );
        let has_go_mod = files.iter().any(|f| f.path.as_path() == Path::new("go.mod"));
        let has_pom_xml = files.iter().any(|f| f.path.as_path() == Path::new("pom.xml"));
        let has_build_gradle = files.iter().any(|f| 
            f.path.as_path() == Path::new("build.gradle") ||
            f.path.as_path() == Path::new("build.gradle.kts")
        );
        
        // 优先检测明确的项目类型
        if has_cargo_toml {
            return ProjectType::Rust;
        }
        if has_package_json {
            return ProjectType::Node;
        }
        if has_go_mod {
            return ProjectType::Go;
        }
        if has_pyproject {
            return ProjectType::Python;
        }
        if has_pom_xml || has_build_gradle {
            return ProjectType::Java;
        }
        
        // 根据源文件类型推断
        let rust_count = files.iter().filter(|f| f.language == Language::Rust).count();
        let js_count = files.iter().filter(|f| 
            f.language == Language::JavaScript || f.language == Language::TypeScript
        ).count();
        let py_count = files.iter().filter(|f| f.language == Language::Python).count();
        let go_count = files.iter().filter(|f| f.language == Language::Go).count();
        let java_count = files.iter().filter(|f| 
            f.language == Language::Java || f.language == Language::Kotlin
        ).count();
        
        let max = rust_count.max(js_count).max(py_count).max(go_count).max(java_count);
        
        if max == 0 {
            return ProjectType::Unknown;
        }
        
        // 如果有多种语言且数量接近，标记为混合项目
        let threshold = (max as f64 * 0.3) as usize;
        let lang_types = [
            rust_count > threshold,
            js_count > threshold,
            py_count > threshold,
            go_count > threshold,
            java_count > threshold,
        ].iter().filter(|&&x| x).count();
        
        if lang_types > 1 {
            return ProjectType::Mixed;
        }
        
        if max == rust_count {
            ProjectType::Rust
        } else if max == js_count {
            ProjectType::Node
        } else if max == py_count {
            ProjectType::Python
        } else if max == go_count {
            ProjectType::Go
        } else if max == java_count {
            ProjectType::Java
        } else {
            ProjectType::Unknown
        }
    }
}

impl ProjectInfo {
    /// 获取主要语言
    pub fn primary_language(&self) -> Option<Language> {
        self.languages.first().copied()
    }
    
    /// 获取指定语言的文件列表
    pub fn files_by_language(&self, lang: Language) -> Vec<&FileInfo> {
        self.files.iter().filter(|f| f.language == lang).collect()
    }
    
    /// 格式化为简洁的项目摘要
    pub fn summary(&self) -> String {
        let lang_str = self.languages.iter()
            .take(3)
            .map(|l| l.display_name())
            .collect::<Vec<_>>()
            .join(", ");
        
        format!(
            "{} ({}) - {} 文件, {} 源文件, 语言: {}",
            self.name,
            self.project_type.display_name(),
            self.total_files,
            self.source_files_count,
            lang_str
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
        fs::write(dir.path().join("src/lib.rs"), "pub fn test() {}").unwrap();
        
        dir
    }
    
    #[test]
    fn test_scan_rust_project() {
        let dir = create_test_project();
        let scanner = ProjectScanner::new(dir.path().to_path_buf());
        let info = scanner.scan().unwrap();
        
        assert_eq!(info.project_type, ProjectType::Rust);
        assert!(info.languages.contains(&Language::Rust));
        assert!(info.source_files_count >= 2);
    }
    
    #[test]
    fn test_project_summary() {
        let dir = create_test_project();
        let scanner = ProjectScanner::new(dir.path().to_path_buf());
        let info = scanner.scan().unwrap();
        
        let summary = info.summary();
        assert!(summary.contains("Rust"));
    }
}
