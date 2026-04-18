//! 工具宏 - 简化工具定义
//!
//! 提供宏来自动实现 Tool trait

/// 定义工具的宏
///
/// 示例：
/// ```rust
/// define_tool! {
///     name: "read_file",
///     description: "读取文件内容",
///     category: File,
///     danger: 0,
///     params: [{"name": "path", "type": "string", "required": true}],
///     execute: |args| async move {
///         // 执行逻辑
///         Ok("result".to_string())
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_tool {
    (
        name: $name:expr,
        description: $desc:expr,
        category: $cat:ident,
        danger: $danger:expr,
        primary_param: $primary:expr,
        result_display: $display:ident,
        params: $schema:expr,
        execute: $exec:expr
    ) => {
        use async_trait::async_trait;

        pub struct ToolImpl;

        impl ToolImpl {
            pub fn new() -> Self { Self }
        }

        impl Default for ToolImpl {
            fn default() -> Self { Self::new() }
        }

        #[async_trait]
        impl $crate::Tool for ToolImpl {
            fn metadata(&self) -> $crate::ToolMetadata {
                $crate::ToolMetadata {
                    name: $name,
                    description: $desc,
                    category: $crate::ToolCategory::$cat,
                    danger_level: $danger,
                    display: $crate::ToolDisplayConfig {
                        primary_param: $primary,
                        result_display: $crate::ResultDisplayType::$display,
                        max_preview_lines: 10,
                        max_preview_chars: 500,
                    },
                }
            }

            fn parameters_schema(&self) -> serde_json::Value {
                $schema
            }

            async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
                $exec(args).await
            }
        }
    };
}
