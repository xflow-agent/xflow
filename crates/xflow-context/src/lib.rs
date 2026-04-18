//! 智能上下文管理
//!
//! 提供：
//! - 项目结构扫描
//! - 语言检测
//! - Token 估算
//! - 上下文构建

mod context;
mod language;
mod scanner;
mod token_estimator;

pub use context::{ContextBuilder, ProjectContext};
pub use language::{detect_language, is_source_file, Language};
pub use scanner::{FileInfo, ProjectInfo, ProjectScanner};
pub use token_estimator::{estimate_tokens, TokenEstimator};
