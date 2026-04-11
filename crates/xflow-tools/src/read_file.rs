//! read_file 工具实现

use super::tool::Tool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

/// read_file 工具参数
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadFileArgs {
    /// 文件路径
    pub path: String,
}

/// read_file 工具
pub struct ReadFileTool {
    #[allow(dead_code)]
    workdir: PathBuf,
}

impl ReadFileTool {
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

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "读取文件内容。参数: path - 要读取的文件路径（绝对路径或相对路径）"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要读取的文件路径"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let params: ReadFileArgs = serde_json::from_value(args)?;
        let path = PathBuf::from(&params.path);

        debug!("读取文件: {:?}", path);

        // 检查文件是否存在
        if !path.exists() {
            warn!("文件不存在: {:?}", path);
            return Ok(format!("错误: 文件不存在: {}", params.path));
        }

        // 检查是否为文件
        if !path.is_file() {
            return Ok(format!("错误: {} 不是文件", params.path));
        }

        // 读取文件内容
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let lines = content.lines().count();
                Ok(format!(
                    "文件: {} ({} 行)\n---\n{}\n---",
                    params.path, lines, content
                ))
            }
            Err(e) => {
                warn!("读取文件失败: {:?}", e);
                Ok(format!("错误: 无法读取文件 {}: {}", params.path, e))
            }
        }
    }
}
