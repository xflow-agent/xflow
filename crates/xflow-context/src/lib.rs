//! 智能上下文管理
//!
//! 提供：
//! - 项目结构扫描
//! - 语言检测
//! - Token 估算
//! - 上下文构建

mod scanner;
mod language;
mod token_estimator;
mod context;

pub use scanner::{ProjectScanner, ProjectInfo, FileInfo};
pub use language::{Language, detect_language, is_source_file};
pub use token_estimator::{TokenEstimator, estimate_tokens};
pub use context::{ContextBuilder, ProjectContext};