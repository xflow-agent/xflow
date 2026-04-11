//! 会话管理

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_model::{Message, ModelProvider, StreamChunk, ToolCall, ToolDefinition};
use xflow_tools::ToolRegistry;
use xflow_context::ContextBuilder;
use xflow_agent::{ReviewerAgent, CoderAgent, Agent, AgentContext, Task, TaskType, TaskStatus};

/// 最大工具调用循环次数
const MAX_TOOL_LOOPS: usize = 20;

/// 需要确认的工具列表
const TOOLS_REQUIRING_CONFIRMATION: &[&str] = &["write_file", "run_shell"];

/// 系统提示词（基础部分）
const SYSTEM_PROMPT_BASE: &str = r#"你是一个智能编程助手 xflow (心流)。你可以使用工具来帮助用户完成编程任务。

## 基础工具

- read_file: 读取单个文件内容
- write_file: 写入文件（需用户确认）
- list_directory: 列出目录内容
- search_file: 搜索代码
- run_shell: 执行 Shell 命令（需用户确认）

## 高级工具（重要！）

### analyze_project - 项目分析工具

**当用户说以下内容时，必须调用此工具，不要直接用 read_file：**
- "分析项目"、"分析这个项目"、"分析所有功能"
- "项目架构是什么"、"项目结构"
- "了解这个项目"、"理解项目"

**示例：**
用户: "分析一下这个项目的所有功能"
正确做法: 调用 analyze_project(task="分析项目所有功能")
错误做法: 直接调用 read_file(Cargo.toml) 然后输出结论

### implement_feature - 功能实现工具

**当用户要求实现新功能时，调用此工具：**
- "实现xxx功能"、"添加xxx功能"
- "创建xxx"、"编写xxx"

## 工作原则

1. **识别任务类型**：先判断是"分析"还是"实现"还是"简单操作"
2. **选择正确工具**：复杂任务用高级工具，简单任务用基础工具
3. **完整执行**：不要中途停止，要完成所有必要的步骤
4. **自动循环**：你会自动循环执行直到任务完全完成

## 任务类型判断

| 用户请求 | 正确做法 |
|---------|---------|
| "分析项目功能" | 调用 analyze_project |
| "实现登录功能" | 调用 implement_feature |
| "读取 main.rs" | 直接调用 read_file |
| "执行 cargo build" | 直接调用 run_shell |

## 重要提醒

- ❌ 不要在用户说"分析项目"时，只读取 Cargo.toml 就输出结论
- ✅ 必须调用 analyze_project 工具执行完整分析流程
- ❌ 不要只建议用户做什么，而是直接执行
- ✅ 完成所有必要的步骤后再输出结果"#;

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
                    } else if tool_name == "analyze_project" {
                        // 特殊处理：调用 Reviewer Agent
                        self.execute_reviewer_agent(tool_args.clone()).await
                    } else if tool_name == "implement_feature" {
                        // 特殊处理：调用 Coder Agent
                        self.execute_coder_agent(tool_args.clone()).await
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

    /// 执行 Reviewer Agent（项目分析）
    async fn execute_reviewer_agent(&self, args: serde_json::Value) -> String {
        let task_desc = args.get("task")
            .and_then(|t| t.as_str())
            .unwrap_or("分析项目");

        info!("执行 Reviewer Agent: {}", task_desc);
        println!("\n📊 开始项目分析...");

        // 创建 Agent
        let reviewer = ReviewerAgent::new();
        
        // 创建任务
        let task = Task {
            id: format!("review-{:?}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
            description: task_desc.to_string(),
            task_type: TaskType::Analysis,
            subtasks: vec![],
            status: TaskStatus::Pending,
            priority: 5,
            dependencies: vec![],
        };

        // Agent 工具循环（只调用基础工具，不调用其它 Agent）
        let mut context = AgentContext::new(self.workdir.clone());
        let max_iterations = 10;
        let mut iteration = 0;
        
        loop {
            if iteration >= max_iterations {
                warn!("Agent 达到最大迭代次数");
                break;
            }
            
            // 调用 Agent
            match reviewer.execute(&task, &context, self.provider.clone()).await {
                Ok(response) => {
                    // 打印输出
                    if !response.output.is_empty() {
                        println!("{}", response.output);
                    }
                    
                    // 如果没有工具调用，返回结果
                    if response.tool_calls.is_empty() {
                        return response.output;
                    }
                    
                    // 执行工具调用
                    for tool_call in &response.tool_calls {
                        let tool_name = &tool_call.name;
                        let tool_args = &tool_call.arguments;
                        
                        // 跳过 Agent 工具（Agent 不能调用其它 Agent）
                        if tool_name == "analyze_project" || tool_name == "implement_feature" {
                            println!("[跳过 Agent 工具调用: {}]", tool_name);
                            continue;
                        }
                        
                        println!("\n[Agent 调用工具: {}]", tool_name);
                        
                        // 执行基础工具
                        let result = if let Some(tool) = self.tools.get(tool_name) {
                            match tool.execute(tool_args.clone()).await {
                                Ok(r) => r,
                                Err(e) => format!("错误: {}", e),
                            }
                        } else {
                            format!("未知工具: {}", tool_name)
                        };
                        
                        println!("[结果: {} 字节]", result.len());
                        
                        // 将结果添加到上下文
                        context.add_tool_result(xflow_agent::ToolResult {
                            name: tool_name.clone(),
                            result,
                            success: true,
                        });
                    }
                    
                    iteration += 1;
                }
                Err(e) => {
                    warn!("Agent 执行失败: {}", e);
                    return format!("分析失败: {}", e);
                }
            }
        }
        
        "分析完成".to_string()
    }

    /// 执行 Coder Agent（功能实现）
    async fn execute_coder_agent(&self, args: serde_json::Value) -> String {
        let task_desc = args.get("task")
            .and_then(|t| t.as_str())
            .unwrap_or("实现功能");

        info!("执行 Coder Agent: {}", task_desc);
        println!("\n🔧 开始功能实现...");

        // 创建 Agent 和上下文
        let coder = CoderAgent::new();
        
        // 创建任务
        let task = Task {
            id: format!("code-{:?}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
            description: task_desc.to_string(),
            task_type: TaskType::Coding,
            subtasks: vec![],
            status: TaskStatus::Pending,
            priority: 5,
            dependencies: vec![],
        };

        // Agent 工具循环（只调用基础工具，不调用其它 Agent）
        let mut context = AgentContext::new(self.workdir.clone());
        let max_iterations = 10;
        let mut iteration = 0;
        
        loop {
            if iteration >= max_iterations {
                warn!("Agent 达到最大迭代次数");
                break;
            }
            
            // 调用 Agent
            match coder.execute(&task, &context, self.provider.clone()).await {
                Ok(response) => {
                    // 打印输出
                    if !response.output.is_empty() {
                        println!("{}", response.output);
                    }
                    
                    // 如果没有工具调用，返回结果
                    if response.tool_calls.is_empty() {
                        return response.output;
                    }
                    
                    // 执行工具调用
                    for tool_call in &response.tool_calls {
                        let tool_name = &tool_call.name;
                        let tool_args = &tool_call.arguments;
                        
                        // 跳过 Agent 工具（Agent 不能调用其它 Agent）
                        if tool_name == "analyze_project" || tool_name == "implement_feature" {
                            println!("[跳过 Agent 工具调用: {}]", tool_name);
                            continue;
                        }
                        
                        println!("\n[Agent 调用工具: {}]", tool_name);
                        
                        // 执行基础工具
                        let result = if let Some(tool) = self.tools.get(tool_name) {
                            match tool.execute(tool_args.clone()).await {
                                Ok(r) => r,
                                Err(e) => format!("错误: {}", e),
                            }
                        } else {
                            format!("未知工具: {}", tool_name)
                        };
                        
                        println!("[结果: {} 字节]", result.len());
                        
                        // 将结果添加到上下文
                        context.add_tool_result(xflow_agent::ToolResult {
                            name: tool_name.clone(),
                            result,
                            success: true,
                        });
                    }
                    
                    iteration += 1;
                }
                Err(e) => {
                    warn!("Agent 执行失败: {}", e);
                    return format!("实现失败: {}", e);
                }
            }
        }
        
        "实现完成".to_string()
    }
}
