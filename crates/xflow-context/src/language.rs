//! 语言检测和文件类型识别

use std::path::Path;

/// 支持的编程语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    // 主要编程语言
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    ObjectiveC,
    Scala,
    Haskell,
    Lua,
    Perl,
    R,
    Zig,
    Elixir,
    Erlang,
    Clojure,
    Dart,
    
    // 配置/标记语言
    Json,
    Yaml,
    Toml,
    Xml,
    Html,
    Css,
    Markdown,
    
    // Shell
    Shell,
    Bash,
    Zsh,
    Fish,
    PowerShell,
    
    // 其他
    Sql,
    Dockerfile,
    Makefile,
    GitIgnore,
    Unknown,
}

impl Language {
    /// 获取语言的显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Python => "Python",
            Language::Go => "Go",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::CSharp => "C#",
            Language::Ruby => "Ruby",
            Language::Php => "PHP",
            Language::Swift => "Swift",
            Language::ObjectiveC => "Objective-C",
            Language::Scala => "Scala",
            Language::Haskell => "Haskell",
            Language::Lua => "Lua",
            Language::Perl => "Perl",
            Language::R => "R",
            Language::Zig => "Zig",
            Language::Elixir => "Elixir",
            Language::Erlang => "Erlang",
            Language::Clojure => "Clojure",
            Language::Dart => "Dart",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Xml => "XML",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Markdown => "Markdown",
            Language::Shell => "Shell",
            Language::Bash => "Bash",
            Language::Zsh => "Zsh",
            Language::Fish => "Fish",
            Language::PowerShell => "PowerShell",
            Language::Sql => "SQL",
            Language::Dockerfile => "Dockerfile",
            Language::Makefile => "Makefile",
            Language::GitIgnore => ".gitignore",
            Language::Unknown => "Unknown",
        }
    }
    
    /// 是否是主要编程语言
    pub fn is_programming_language(&self) -> bool {
        matches!(self,
            Language::Rust | Language::TypeScript | Language::JavaScript |
            Language::Python | Language::Go | Language::Java | Language::Kotlin |
            Language::C | Language::Cpp | Language::CSharp |
            Language::Ruby | Language::Php | Language::Swift |
            Language::Scala | Language::Haskell | Language::Lua |
            Language::Perl | Language::R | Language::Zig |
            Language::Elixir | Language::Erlang | Language::Clojure | Language::Dart
        )
    }
    
    /// 获取文件的注释前缀
    pub fn comment_prefix(&self) -> &'static str {
        match self {
            Language::Rust | Language::C | Language::Cpp | Language::Go |
            Language::Java | Language::JavaScript | Language::TypeScript |
            Language::CSharp | Language::Swift | Language::Kotlin |
            Language::Scala | Language::Dart => "//",
            Language::Python | Language::Ruby | Language::Shell |
            Language::Bash | Language::Zsh | Language::Fish |
            Language::Perl | Language::R | Language::Yaml |
            Language::Toml => "#",
            Language::Sql => "--",
            Language::Lua => "--",
            Language::Haskell => "--",
            Language::Elixir | Language::Erlang | Language::Clojure => "#",
            _ => "#",
        }
    }
}

/// 根据文件扩展名检测语言
pub fn detect_language(path: &Path) -> Language {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    
    let filename = path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase())
        .unwrap_or_default();
    
    // 先检查特殊文件名
    match filename.as_str() {
        "dockerfile" => return Language::Dockerfile,
        "makefile" | "gnumakefile" => return Language::Makefile,
        ".gitignore" => return Language::GitIgnore,
        _ => {}
    }
    
    // 根据扩展名判断
    match ext.as_str() {
        // Rust
        "rs" => Language::Rust,
        
        // TypeScript/JavaScript
        "ts" | "tsx" => Language::TypeScript,
        "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
        
        // Python
        "py" | "pyi" | "pyw" => Language::Python,
        
        // Go
        "go" => Language::Go,
        
        // Java/Kotlin
        "java" => Language::Java,
        "kt" | "kts" => Language::Kotlin,
        
        // C/C++
        "c" | "h" => Language::C,
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Language::Cpp,
        
        // C#
        "cs" => Language::CSharp,
        
        // Ruby
        "rb" | "rake" | "gemspec" => Language::Ruby,
        
        // PHP
        "php" => Language::Php,
        
        // Swift/Objective-C
        "swift" => Language::Swift,
        "m" | "mm" => Language::ObjectiveC,
        
        // Scala
        "scala" | "sc" => Language::Scala,
        
        // Haskell
        "hs" | "lhs" => Language::Haskell,
        
        // Lua
        "lua" => Language::Lua,
        
        // Perl
        "pl" | "pm" | "t" => Language::Perl,
        
        // R
        "r" | "rmd" => Language::R,
        
        // Zig
        "zig" => Language::Zig,
        
        // Elixir
        "ex" | "exs" => Language::Elixir,
        
        // Erlang
        "erl" | "hrl" => Language::Erlang,
        
        // Clojure
        "clj" | "cljs" | "cljc" => Language::Clojure,
        
        // Dart
        "dart" => Language::Dart,
        
        // JSON
        "json" => Language::Json,
        
        // YAML
        "yaml" | "yml" => Language::Yaml,
        
        // TOML
        "toml" => Language::Toml,
        
        // XML
        "xml" | "xsl" | "xslt" | "svg" => Language::Xml,
        
        // HTML
        "html" | "htm" | "xhtml" => Language::Html,
        
        // CSS
        "css" | "scss" | "sass" | "less" => Language::Css,
        
        // Markdown
        "md" | "markdown" => Language::Markdown,
        
        // Shell
        "sh" => Language::Bash,
        "zsh" => Language::Zsh,
        "fish" => Language::Fish,
        "ps1" | "psm1" | "psd1" => Language::PowerShell,
        
        // SQL
        "sql" => Language::Sql,
        
        _ => Language::Unknown,
    }
}

/// 判断文件是否是源代码文件
pub fn is_source_file(path: &Path) -> bool {
    let lang = detect_language(path);
    lang.is_programming_language() || matches!(lang,
        Language::Json | Language::Yaml | Language::Toml |
        Language::Xml | Language::Html | Language::Css |
        Language::Markdown | Language::Dockerfile | Language::Makefile |
        Language::Sql
    )
}

/// 判断文件是否应该被忽略
pub fn should_ignore(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let path_lower = path_str.to_lowercase();
    
    // 常见忽略目录
    let ignore_dirs = [
        "node_modules",
        "target",
        "build",
        "dist",
        "vendor",
        ".git",
        ".svn",
        ".hg",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        "venv",
        ".venv",
        "env",
        ".env",
        " Pods",
        ".gradle",
        ".idea",
        ".vscode",
        ".settings",
        "bin",
        "obj",
        "pkg",
        "out",
        "log",
        "logs",
        "tmp",
        "temp",
        ".next",
        ".nuxt",
        "coverage",
        ".nyc_output",
    ];
    
    for dir in &ignore_dirs {
        if path_lower.contains(dir) {
            return true;
        }
    }
    
    // 常见忽略文件
    let filename = path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase())
        .unwrap_or_default();
    
    let ignore_files = [
        ".ds_store",
        "thumbs.db",
        ".env",
        ".env.local",
        ".env.development",
        ".env.production",
        "*.lock",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "cargo.lock",
        "poetry.lock",
        "composer.lock",
        "gemfile.lock",
        "pipfile.lock",
    ];
    
    for file in &ignore_files {
        if file.starts_with('*') {
            let suffix = &file[1..];
            if filename.ends_with(suffix) {
                return true;
            }
        } else if filename == *file {
            return true;
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_detect_rust() {
        assert_eq!(detect_language(PathBuf::from("main.rs").as_path()), Language::Rust);
        assert_eq!(detect_language(PathBuf::from("lib.rs").as_path()), Language::Rust);
    }
    
    #[test]
    fn test_detect_typescript() {
        assert_eq!(detect_language(PathBuf::from("app.ts").as_path()), Language::TypeScript);
        assert_eq!(detect_language(PathBuf::from("component.tsx").as_path()), Language::TypeScript);
    }
    
    #[test]
    fn test_detect_python() {
        assert_eq!(detect_language(PathBuf::from("main.py").as_path()), Language::Python);
        assert_eq!(detect_language(PathBuf::from("__init__.py").as_path()), Language::Python);
    }
    
    #[test]
    fn test_detect_dockerfile() {
        assert_eq!(detect_language(PathBuf::from("Dockerfile").as_path()), Language::Dockerfile);
        assert_eq!(detect_language(PathBuf::from("dockerfile").as_path()), Language::Dockerfile);
    }
    
    #[test]
    fn test_is_source_file() {
        assert!(is_source_file(PathBuf::from("main.rs").as_path()));
        assert!(is_source_file(PathBuf::from("config.json").as_path()));
        assert!(!is_source_file(PathBuf::from("image.png").as_path()));
    }
}
