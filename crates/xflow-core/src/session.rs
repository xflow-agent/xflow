//! 会话管理

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_model::{Message, ModelProvider, StreamChunk, ToolCall, ToolDefinition};
use xflow_tools::ToolRegistry;
use xflow_context::ContextBuilder;

/// 最大工具调用循环次数
const MAX_TOOL_LOOPS: usize = 20;

/// 需要确认的工具列表
const TOOLS_REQUIRING_CONFIRMATION: &[&str] = &["write_file", "run_shell"];

/// 系统提示词（基础部分）
const SYSTEM_PROMPT_BASE: &str = r#"你是一个智能编程助手 xflow (心流)。你可以使用工具来帮助用户完成编程任务。

## 你的能力

你可以使用以下工具：
- read_file: 读取文件内容
- write_file: 写入文件（需用户确认）
- list_directory: 列出目录内容
- search_file: 搜索代码（使用 ripgrep）
- run_shell: 执行 Shell 命令（需用户确认）

## 工作原则

1. **完整执行**: 当用户给你一个多步骤任务时，你必须执行**所有步骤**，不能只做一部分就停止。
2. **自动循环**: 你会自动循环执行直到任务完全完成。不要在中间步骤停止。
3. **及时汇报**: 简要说明你在做什么，让用户了解进度。
4. **安全意识**: 对于危险操作（写入文件、执行命令），系统会要求用户确认。

## 重要：多步骤任务示例

示例1: 用户说 "在 /tmp/test 目录创建 Rust 项目并运行"
你需要执行以下**所有步骤**：
1. run_shell: mkdir -p /tmp/test
2. run_shell: cd /tmp/test && cargo init
3. write_file: 写入 src/main.rs 内容
4. run_shell: cd /tmp/test && cargo run
5. 报告运行结果

示例2: 用户说 "修复所有编译错误"
你需要执行以下**所有步骤**：
1. run_shell: cargo check 查看错误
2. read_file: 读取有错误的文件
3. write_file: 修复代码
4. run_shell: cargo check 再次检查
5. 如果还有错误，继续修复；如果没有，报告成功

## 关键原则

- **不要中途停止**: 即使完成了一个步骤，如果还有后续步骤，必须继续执行
- **不要只是建议**: 不要说"你可以..."，而是直接执行
- **检查结果**: 每个步骤完成后，检查是否需要继续

记住：任务是"创建项目并运行"，不是"创建项目"。"并"意味着所有步骤都要完成。"#;

/// 会话状态
pub struct Session {
    /// 消息历史
    messages: Vec<Message>,
    /// 模型提供者
    provider: Arc<dyn ModelProvider>,
    /// 工作目录
    workdir: PathBuf,
    /// 模型名称
    model_name: String,
    /// 工具注册表
    tools: ToolRegistry,
    /// 是否自动确认（跳过确认对话框）
    auto_confirm: bool,
    /// 是否已添加系统提示词
    system_added: bool,
    /// 项目上下文信息（可选）
    project_context: Option<String>,
}

impl Session {
    /// 创建新会话
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf) -> Self {
        let model_name = provider.model_info().name;
        let tools = xflow_tools::create_default_tools();
        Self {
            messages: Vec::new(),
            provider,
            workdir,
            model_name,
            tools,
            auto_confirm: false,
            system_added: false,
            project_context: None,
        }
    }

    /// 初始化项目上下文（扫描项目并生成上下文信息）
    pub fn init_project_context(&mut self) -> Result<()> {
        info!("初始化项目上下文: {:?}", self.workdir);
        
        let builder = ContextBuilder::new(self.workdir.clone());
        match builder.generate_system_context() {
            Ok(context) => {
                info!("项目上下文初始化成功");
                println!("📁 正在扫描项目目录...");
                if let Ok(proj_info) = builder.build() {
                    println!("   {}", proj_info.info.summary());
                }
                self.project_context = Some(context);
            }
            Err(e) => {
                warn!("项目上下文初始化失败: {}", e);
            }
        }
        
        Ok(())
    }

    /// 设置自动确认模式
    pub fn set_auto_confirm(&mut self, auto: bool) {
        self.auto_confirm = auto;
    }

    /// 获取工具定义列表
    fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .definitions()
            .into_iter()
            .map(|td| xflow_model::ToolDefinition {
                tool_type: td.tool_type,
                function: xflow_model::FunctionDefinition {
                    name: td.function.name,
                    description: td.function.description,
                    parameters: td.function.parameters,
                },
            })
            .collect()
    }

    /// 检查工具是否需要确认
    fn needs_confirmation(&self, tool_name: &str) -> bool {
        TOOLS_REQUIRING_CONFIRMATION.contains(&tool_name)
    }

    /// 请求用户确认
    fn request_confirmation(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        // 自动确认模式
        if self.auto_confirm {
            return true;
        }

        // 格式化参数显示
        let (args_display, danger_info) = match tool_name {
            "write_file" => {
                if let Some(path) = args.get("path") {
                    if let Some(content) = args.get("content") {
                        let content_str = content.as_str().unwrap_or("");
                        let preview = if content_str.len() > 100 {
                            format!("{}...", &content_str[..100])
                        } else {
                            content_str.to_string()
                        };
                        (format!("路径: {}\n内容预览: {}", path, preview), None)
                    } else {
                        (format!("路径: {}", path), None)
                    }
                } else {
                    (args.to_string(), None)
                }
            }
            "run_shell" => {
                if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                    // 分析命令危险程度
                    let analysis = xflow_tools::analyze_command(cmd);
                    let danger_info = if analysis.is_dangerous {
                        Some((analysis.level, analysis.reason.clone()))
                    } else {
                        None
                    };
                    (format!("命令: {}", cmd), danger_info)
                } else {
                    (args.to_string(), None)
                }
            }
            _ => (args.to_string(), None),
        };

        // 显示确认对话框
        println!("\n{}", "=".repeat(50));
        
        // 如果是危险命令，显示警告
        if let Some((level, reason)) = &danger_info {
            let level_display = match level {
                3 => "🔴 极度危险",
                2 => "🟠 高度危险",
                1 => "🟡 中度危险",
                _ => "⚠️ 需要注意",
            };
            println!("{} - {}", level_display, reason);
        } else {
            println!("⚠️  需要确认操作");
        }
        
        println!("{}", "=".repeat(50));
        println!("工具: {}", tool_name);
        println!("{}", args_display);
        println!("{}", "=".repeat(50));

        // 使用 inquire 进行确认
        let confirm_msg = if danger_info.is_some() {
            "⚠️  确认执行此危险操作?"
        } else {
            "是否执行此操作?"
        };
        
        match inquire::Confirm::new(confirm_msg)
            .with_default(false)
            .prompt()
        {
            Ok(true) => {
                println!("✓ 已确认，执行操作...");
                true
            }
            Ok(false) => {
                println!("✗ 已取消");
                false
            }
            Err(e) => {
                warn!("确认对话框错误: {}", e);
                println!("✗ 确认失败，已取消");
                false
            }
        }
    }

    /// 处理用户输入
    pub async fn process(&mut self, input: &str) -> Result<()> {
        // 首次对话时添加系统提示词
        if !self.system_added {
            let system_prompt = if let Some(ref context) = self.project_context {
                format!("{}\n{}", SYSTEM_PROMPT_BASE, context)
            } else {
                SYSTEM_PROMPT_BASE.to_string()
            };
            self.messages.push(Message::system(&system_prompt));
            self.system_added = true;
        }

        // 添加用户消息
        self.messages.push(Message::user(input));

        debug!("当前消息数量: {}", self.messages.len());

        // 工具调用循环
        let mut loop_count = 0;
        let mut total_tools_called = 0;

        loop {
            if loop_count >= MAX_TOOL_LOOPS {
                warn!("达到最大工具调用循环次数: {}", MAX_TOOL_LOOPS);
                println!("\n⚠️ 已达到最大循环次数 ({}), 停止自动执行", MAX_TOOL_LOOPS);
                println!("💡 你可以继续对话，让模型完成剩余任务");
                break;
            }

            // 显示循环进度（非首次）
            if loop_count > 0 {
                println!("\n── 自动执行 (第 {}/{} 轮) ──", loop_count + 1, MAX_TOOL_LOOPS);
            }

            // 调用模型（流式 + 工具）
            let tool_defs = self.get_tool_definitions();
            let stream = self
                .provider
                .chat_stream_with_tools(self.messages.clone(), tool_defs)
                .await;

            // 处理流式响应
            let mut full_response = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

            use futures::StreamExt;
            let mut stream = stream;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(StreamChunk {
                        content,
                        done,
                        tool_calls: chunk_tool_calls,
                    }) => {
                        // 输出文本内容
                        if !content.is_empty() {
                            print!("{}", content);
                            full_response.push_str(&content);
                        }

                        // 收集工具调用
                        if !chunk_tool_calls.is_empty() {
                            tool_calls.extend(chunk_tool_calls);
                        }

                        if done {
                            println!(); // 换行
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("流式响应错误: {}", e);
                        println!("\n[错误: {}]", e);
                        return Err(e.into());
                    }
                }
            }

            // 如果有工具调用，执行并继续循环
            if !tool_calls.is_empty() {
                debug!("收到 {} 个工具调用", tool_calls.len());

                // 添加助手消息（包含工具调用）
                self.messages
                    .push(Message::assistant_with_tools(tool_calls.clone()));

                // 执行每个工具调用
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    let tool_name = &tool_call.function.name;
                    let tool_args = &tool_call.function.arguments;

                    // 工具调用进度
                    if tool_calls.len() > 1 {
                        println!("\n[调用工具 {}/{}: {}]", i + 1, tool_calls.len(), tool_name);
                    } else {
                        println!("\n[调用工具: {}]", tool_name);
                    }
                    debug!("工具参数: {}", tool_args);

                    // 检查是否需要确认
                    let confirmed = if self.needs_confirmation(tool_name) {
                        self.request_confirmation(tool_name, tool_args)
                    } else {
                        true
                    };

                    // 执行工具
                    let result = if !confirmed {
                        "操作已取消".to_string()
                    } else if let Some(tool) = self.tools.get(tool_name) {
                        match tool.execute(tool_args.clone()).await {
                            Ok(result) => result,
                            Err(e) => format!("工具执行错误: {}", e),
                        }
                    } else {
                        format!("未知工具: {}", tool_name)
                    };

                    // 显示结果摘要 (安全截断，避免 UTF-8 边界问题)
                    let result_preview = if result.chars().count() > 200 {
                        format!("{}...", result.chars().take(200).collect::<String>())
                    } else {
                        result.clone()
                    };
                    println!("[结果: {} 字节]", result.len());
                    debug!("工具结果: {}", result_preview);

                    // 添加工具结果消息
                    self.messages
                        .push(Message::tool_result(tool_name, result));
                    
                    total_tools_called += 1;
                }

                loop_count += 1;
                continue; // 继续循环，让模型处理工具结果
            }

            // 没有工具调用，添加助手消息并结束
            if !full_response.is_empty() {
                self.messages.push(Message::assistant(&full_response));
            }

            // 显示执行统计
            if total_tools_called > 0 {
                println!("\n✅ 任务完成 (共调用 {} 次工具, {} 轮循环)", 
                    total_tools_called, loop_count);
            }

            break;
        }

        Ok(())
    }

    /// 清空会话
    pub fn clear(&mut self) {
        self.messages.clear();
        self.system_added = false;
        info!("会话已清空");
    }

    /// 获取模型名称
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 获取工作目录
    #[allow(dead_code)]
    pub fn workdir(&self) -> &PathBuf {
        &self.workdir
    }
}
