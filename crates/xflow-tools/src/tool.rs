//! 工具 Trait 定义

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
pub use xflow_model::{FunctionDefinition, ToolDefinition};

/// 工具类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// 文件操作
    File,
    /// Git 操作
    Git,
    /// Shell 命令
    Shell,
    /// 搜索
    Search,
    /// Agent 工具
    Agent,
    /// 其他
    Other,
}

impl ToolCategory {
    /// 获取显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            ToolCategory::File => "文件工具",
            ToolCategory::Git => "Git 工具",
            ToolCategory::Shell => "Shell 工具",
            ToolCategory::Search => "搜索工具",
            ToolCategory::Agent => "Agent 工具",
            ToolCategory::Other => "其他工具",
        }
    }
}

/// 工具结果展示类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultDisplayType {
    /// 完整内容
    Full,
    /// 摘要
    Summary,
    /// 行数统计
    LineCount,
    /// 字节大小
    ByteSize,
    /// 仅状态
    StatusOnly,
}

/// 工具展示配置
#[derive(Debug, Clone)]
pub struct ToolDisplayConfig {
    /// 主要参数名（用于展示给用户）
    pub primary_param: &'static str,
    /// 结果展示类型
    pub result_display: ResultDisplayType,
    /// 最大预览行数
    pub max_preview_lines: usize,
    /// 最大预览字符数
    pub max_preview_chars: usize,
}

impl Default for ToolDisplayConfig {
    fn default() -> Self {
        Self {
            primary_param: "",
            result_display: ResultDisplayType::Summary,
            max_preview_lines: 10,
            max_preview_chars: 500,
        }
    }
}

/// 工具元数据
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// 工具名称
    pub name: &'static str,
    /// 工具描述
    pub description: &'static str,
    /// 工具类别
    pub category: ToolCategory,
    /// 危险等级 (0-3)，用于提示和默认确认逻辑
    pub danger_level: u8,
    /// 展示配置
    pub display: ToolDisplayConfig,
}

/// 工具调用请求
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    /// 调用 ID
    pub id: String,
    /// 工具类型
    #[serde(rename = "type")]
    pub call_type: String,
    /// 函数调用信息
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionCall {
    /// 函数名称
    pub name: String,
    /// 参数（JSON 字符串）
    pub arguments: String,
}

impl ToolCall {
    /// 解析参数为指定类型
    pub fn parse_args<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<T> {
        let args: T = serde_json::from_str(&self.function.arguments)?;
        Ok(args)
    }
}

/// 确认请求
#[derive(Debug, Clone)]
pub struct ToolConfirmationRequest {
    /// 操作描述
    pub message: String,
    /// 危险等级 (0-3)
    pub danger_level: u8,
    /// 危险原因
    pub danger_reason: Option<String>,
}

impl ToolConfirmationRequest {
    /// 创建新的确认请求
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            danger_level: 0,
            danger_reason: None,
        }
    }

    /// 设置危险等级
    pub fn with_danger(mut self, level: u8, reason: impl Into<String>) -> Self {
        self.danger_level = level;
        self.danger_reason = Some(reason.into());
        self
    }
}

/// 工具 Trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具元数据
    fn metadata(&self) -> ToolMetadata;

    /// 工具名称（从元数据获取）
    fn name(&self) -> &str {
        self.metadata().name
    }

    /// 工具描述（从元数据获取）
    fn description(&self) -> &str {
        self.metadata().description
    }

    /// 参数 JSON Schema
    fn parameters_schema(&self) -> serde_json::Value;

    /// 执行工具
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String>;

    /// 获取工具定义
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters_schema(),
            },
        }
    }

    /// 格式化参数用于展示
    fn format_params(&self, args: &Value) -> String {
        let meta = self.metadata();
        let primary = args
            .get(meta.display.primary_param)
            .and_then(|v| v.as_str())
            .unwrap_or("?");

        match meta.display.primary_param {
            "path" => format!("path=\"{}\"", primary),
            "command" => format!("command=\"{}\"", truncate(primary, 50)),
            "pattern" => format!("pattern=\"{}\"", primary),
            "" => String::new(),
            _ => format!("{}=\"{}\"", meta.display.primary_param, primary),
        }
    }

    /// 格式化结果用于展示
    fn format_result(&self, result: &str) -> (String, usize) {
        let meta = self.metadata();

        match meta.display.result_display {
            ResultDisplayType::Full => (result.to_string(), result.len()),
            ResultDisplayType::LineCount => {
                let lines = result.lines().count();
                let preview: String = result
                    .lines()
                    .take(meta.display.max_preview_lines)
                    .collect::<Vec<_>>()
                    .join("\n");
                (format!("{} ({} 行)", preview, lines), result.len())
            }
            ResultDisplayType::ByteSize => (format_size(result.len()), result.len()),
            ResultDisplayType::StatusOnly => (String::new(), result.len()),
            ResultDisplayType::Summary => {
                if result.len() > meta.display.max_preview_chars {
                    // 安全截断
                    let safe_end = result
                        .char_indices()
                        .take_while(|(idx, _)| *idx < meta.display.max_preview_chars)
                        .last()
                        .map(|(idx, c)| idx + c.len_utf8())
                        .unwrap_or(0);
                    (format!("{}...", &result[..safe_end]), result.len())
                } else {
                    (result.to_string(), result.len())
                }
            }
        }
    }

    /// 构建确认请求
    ///
    /// 默认实现：危险等级 > 0 时需要确认
    /// 工具可以覆盖此方法以：
    /// 1. 自定义确认信息内容
    /// 2. 基于参数动态决定是否确认（返回 Some 或 None）
    /// 3. 完全不需要确认（返回 None）
    fn build_confirmation(&self, args: &Value) -> Option<ToolConfirmationRequest> {
        let meta = self.metadata();

        // 危险等级为 0 时默认不需要确认
        if meta.danger_level == 0 {
            return None;
        }

        // 默认使用参数 JSON 作为消息
        let message = format!("执行 {} 工具\n参数: {}", meta.name, args);

        let req = ToolConfirmationRequest::new(message).with_danger(
            meta.danger_level,
            format!("危险等级: {}", meta.danger_level),
        );

        Some(req)
    }
}

/// 截断字符串
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// 格式化字节大小
fn format_size(size: usize) -> String {
    if size > 1024 * 1024 {
        format!("{:.1}MB", size as f64 / 1024.0 / 1024.0)
    } else if size > 1024 {
        format!("{:.1}KB", size as f64 / 1024.0)
    } else {
        format!("{}B", size)
    }
}
