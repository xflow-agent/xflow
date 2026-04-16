//! 工具 Trait 定义

use async_trait::async_trait;
use serde::Deserialize;
pub use xflow_model::{ToolDefinition, FunctionDefinition};

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