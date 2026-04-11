//! 工具 Trait 定义

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 工具定义（用于告诉模型可用工具）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具类型，固定为 "function"
    #[serde(rename = "type")]
    pub tool_type: String,
    /// 函数信息
    pub function: FunctionDefinition,
}

/// 函数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// 函数名称
    pub name: String,
    /// 函数描述
    pub description: String,
    /// 参数 Schema (JSON Schema)
    pub parameters: serde_json::Value,
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// 调用 ID
    pub tool_call_id: String,
    /// 结果内容
    pub content: String,
    /// 是否成功
    pub success: bool,
}

impl ToolResult {
    /// 创建成功结果
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            success: true,
        }
    }

    /// 创建错误结果
    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: format!("错误: {}", error.into()),
            success: false,
        }
    }
}

/// 工具 Trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

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
}
