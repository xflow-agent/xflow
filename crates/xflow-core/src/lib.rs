//! xflow 核心引擎
//!
//! 负责会话管理、消息处理和工具调用

pub mod config;
mod cli_adapter;
mod events;
mod markdown_renderer;
mod session;
mod ui_adapter;
mod websocket_adapter;

pub use cli_adapter::*;
pub use config::XflowConfig;
pub use events::*;
pub use markdown_renderer::{MarkdownRenderer, StreamingMarkdownRenderer};
pub use session::Session;
pub use ui_adapter::*;
pub use websocket_adapter::*;
pub use xflow_model::{format_io_error, get_reviewer_prompt, get_system_prompt, Message, Role, UserFriendlyError};
