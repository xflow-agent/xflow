//! write_file 工具实现

use super::tool::Tool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

/// write_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct WriteFileArgs {
    /// 文件路径
    pub path: String,
    /// 文件内容
    pub content: String,
}

/// write_file 工具
pub struct WriteFileTool {
    #[allow(dead_code)]
    workdir: PathBuf,
}

impl WriteFileTool {
    /// 创建新实例
    pub fn new() -> Self {
        Self {
            workdir: PathBuf::from("."),
        }
    }

    /// 设置工作目录
    #[allow(dead_code)]
    pub fn with_workdir(mut self, workdir: PathBuf) -> Self {
        self.workdir = workdir;
        self
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "写入文件内容。参数: path - 文件路径, content - 要写入的内容。会覆盖已存在的文件。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要写入的文件路径"
                },
                "content": {
                    "type": "string",
                    "description": "要写入的文件内容"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: WriteFileArgs = serde_json::from_value(args)?;
        let path = PathBuf::from(&params.path);

        debug!("写入文件: {:?}", path);

        // 检查父目录是否存在，不存在则创建
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
                debug!("创建目录: {:?}", parent);
            }
        }

        // 写入文件
        match tokio::fs::write(&path, &params.content).await {
            Ok(_) => {
                let lines = params.content.lines().count();
                Ok(format!(
                    "成功写入文件: {} ({} 行, {} 字节)",
                    params.path,
                    lines,
                    params.content.len()
                ))
            }
            Err(e) => {
                warn!("写入文件失败: {:?}", e);
                Ok(format!("错误: 无法写入文件 {}: {}", params.path, e))
            }
        }
    }
}
