# xflow 架构设计文档

## 模块架构

```
┌─────────────────────────────────────────────────────────────┐
│                        CLI (xflow)                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Clap      │  │  Rustyline  │  │     Formatter       │  │
│  │  (Args)     │  │   (REPL)    │  │   (Output)          │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                     Core (xflow-core)                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Session    │  │  Pipeline   │  │   ToolExecutor      │  │
│  │  (State)    │──│ (Messages)  │──│   (Dispatch)        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────┬───────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
┌───────▼───────┐ ┌───────▼───────┐ ┌───────▼───────┐
│    Model      │ │    Tools      │ │   Context     │
│ (xflow-model) │ │ (xflow-tools) │ │(xflow-context)│
│               │ │               │ │               │
│ ┌───────────┐ │ │ ┌───────────┐ │ │ ┌───────────┐ │
│ │ Ollama    │ │ │ │ FileTools │ │ │ │ ASTParser │ │
│ │ Provider  │ │ │ │ ShellTool │ │ │ │ Selector  │ │
│ └───────────┘ │ │ │ GitTools  │ │ │ │ TokenCnt  │ │
└───────────────┘ │ └───────────┘ │ └───────────┘ │
                  └───────────────┘ └───────────────┘
```

---

## 核心模块详解

### 1. xflow (CLI)

**职责**：用户交互入口

```rust
// src/main.rs
pub struct Cli {
    /// 模型名称
    #[arg(short, long, default_value = "codellama:7b")]
    pub model: String,
    
    /// Ollama 服务地址
    #[arg(long, default_value = "http://localhost:11434")]
    pub host: String,
    
    /// 工作目录
    #[arg(short, long, default_value = ".")]
    pub workdir: PathBuf,
}

// REPL 交互
pub struct Repl {
    editor: Editor<()>,
    session: Session,
}

impl Repl {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            let line = self.editor.readline("xflow> ")?;
            self.session.process(&line).await?;
        }
    }
}
```

### 2. xflow-core

**职责**：核心引擎，协调各模块

```rust
// Session - 会话状态管理
pub struct Session {
    messages: Vec<Message>,
    model: Box<dyn ModelProvider>,
    tools: ToolRegistry,
    context: ContextManager,
}

impl Session {
    pub async fn process(&mut self, input: &str) -> Result<()> {
        // 1. 添加用户消息
        self.messages.push(Message::user(input));
        
        // 2. 构建请求（含上下文）
        let context = self.context.select(input, &self.messages)?;
        
        // 3. 调用模型
        let response = self.model.chat(&self.messages, context).await?;
        
        // 4. 处理响应（可能包含工具调用）
        self.handle_response(response).await?;
        
        Ok(())
    }
    
    async fn handle_response(&mut self, response: Response) -> Result<()> {
        for content in response.content {
            match content {
                Content::Text(text) => {
                    println!("{}", text);
                    self.messages.push(Message::assistant(text));
                }
                Content::ToolCall(call) => {
                    let result = self.tools.execute(&call).await?;
                    self.messages.push(Message::tool_result(call.id, result));
                    // 继续调用模型
                    let next = self.model.chat(&self.messages, None).await?;
                    Box::pin(self.handle_response(next)).await?;
                }
            }
        }
        Ok(())
    }
}

// Pipeline - 消息处理流水线
pub struct MessagePipeline {
    steps: Vec<Box<dyn PipelineStep>>,
}

pub trait PipelineStep {
    fn process(&self, ctx: &mut PipelineContext) -> Result<()>;
}
```

### 3. xflow-model

**职责**：模型接口层，支持多种模型后端

```rust
// ModelProvider Trait - 统一模型接口
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// 发送消息并获取响应
    async fn chat(&self, messages: &[Message], context: Option<Context>) -> Result<Response>;
    
    /// 流式发送消息
    async fn chat_stream(&self, messages: &[Message], context: Option<Context>) 
        -> Result<impl Stream<Item = Result<StreamChunk>>>;
    
    /// 获取模型信息
    fn model_info(&self) -> ModelInfo;
}

// Ollama 实现
pub struct OllamaProvider {
    client: reqwest::Client,
    host: String,
    model: String,
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn chat(&self, messages: &[Message], context: Option<Context>) -> Result<Response> {
        let request = self.build_request(messages, context);
        let response = self.client
            .post(format!("{}/api/chat", self.host))
            .json(&request)
            .send()
            .await?;
        
        self.parse_response(response).await
    }
    
    async fn chat_stream(&self, messages: &[Message], context: Option<Context>) 
        -> Result<impl Stream<Item = Result<StreamChunk>>> {
        // 流式响应实现
    }
}

// Message 格式
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

pub enum Content {
    Text(String),
    ToolCall(ToolCall),
    ToolResult(String),
}

// ToolCall 格式（Ollama 兼容）
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
```

### 4. xflow-tools

**职责**：工具系统实现

```rust
// Tool Trait - 工具统一接口
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;
    
    /// 工具描述
    fn description(&self) -> &str;
    
    /// 参数 JSON Schema
    fn parameters_schema(&self) -> serde_json::Value;
    
    /// 是否需要确认
    fn requires_confirmation(&self) -> bool {
        false
    }
    
    /// 执行工具
    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult>;
}

// 工具结果
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    safety: SafetyGuard,
}

impl ToolRegistry {
    pub async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        let tool = self.tools.get(&call.name)
            .ok_or_else(|| Error::ToolNotFound(call.name.clone()))?;
        
        // 安全检查
        if tool.requires_confirmation() {
            self.safety.confirm(&call.name, &call.arguments)?;
        }
        
        tool.execute(call.arguments.clone()).await
    }
}

// === 具体工具实现 ===

// 文件读取工具
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    
    fn description(&self) -> &str {
        "读取文件内容，支持文本文件、图片等"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "文件的绝对路径"
                }
            },
            "required": ["path"]
        })
    }
    
    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let path: String = params["path"].as_str()
            .ok_or(Error::MissingParameter("path"))?
            .to_string();
        
        let content = tokio::fs::read_to_string(&path).await?;
        
        Ok(ToolResult {
            output: content,
            is_error: false,
        })
    }
}

// Shell 执行工具
pub struct RunShellTool {
    workdir: PathBuf,
}

#[async_trait]
impl Tool for RunShellTool {
    fn name(&self) -> &str { "run_shell" }
    
    fn requires_confirmation(&self) -> bool { true }
    
    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let command: String = params["command"].as_str()
            .ok_or(Error::MissingParameter("command"))?
            .to_string();
        
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&self.workdir)
            .output()
            .await?;
        
        let result = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };
        
        Ok(ToolResult {
            output: result,
            is_error: !output.status.success(),
        })
    }
}
```

### 5. xflow-context

**职责**：智能上下文管理

```rust
// ContextManager
pub struct ContextManager {
    ast_parser: Option<AstParser>,
    project_root: PathBuf,
    max_tokens: usize,
}

impl ContextManager {
    /// 选择相关上下文
    pub fn select(&self, query: &str, history: &[Message]) -> Result<Option<Context>> {
        // 1. 分析查询意图
        let intent = self.analyze_intent(query)?;
        
        // 2. 根据意图选择文件
        let files = self.select_files(&intent)?;
        
        // 3. 构建 Context
        let context = self.build_context(files)?;
        
        // 4. Token 预算检查
        if context.token_count > self.max_tokens {
            return self.truncate_context(context, self.max_tokens);
        }
        
        Ok(Some(context))
    }
    
    fn analyze_intent(&self, query: &str) -> Result<Intent> {
        // 简单关键词匹配或使用小模型分类
        Ok(Intent::CodeModification {
            target_files: vec![],
        })
    }
    
    fn select_files(&self, intent: &Intent) -> Result<Vec<PathBuf>> {
        // 基于意图和 AST 分析选择文件
        Ok(vec![])
    }
}

// AST Parser
pub struct AstParser {
    parsers: HashMap<String, tree_sitter::Parser>,
}

impl AstParser {
    pub fn parse(&self, content: &str, language: &str) -> Result<Tree> {
        let parser = self.parsers.get(language)
            .ok_or(Error::UnsupportedLanguage(language.to_string()))?;
        
        Ok(parser.parse(content, None).unwrap())
    }
}

// Context 结构
pub struct Context {
    pub system_prompt: String,
    pub files: Vec<FileContext>,
    pub token_count: usize,
}

pub struct FileContext {
    pub path: PathBuf,
    pub content: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
}
```

---

## 数据流

### 用户输入处理流程

```
用户输入 "帮我修复编译错误"
         │
         ▼
┌─────────────────────┐
│   CLI (REPL)        │
│  - 接收输入         │
│  - 添加到历史       │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Core (Session)    │
│  - 构建请求         │
│  - 选择上下文       │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Model (Ollama)    │
│  - 发送请求         │
│  - 流式接收响应     │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Core (Response)   │
│  - 解析响应         │
│  - 文本内容输出     │
│  - 工具调用执行     │
└─────────┬───────────┘
          │
          │ Tool Call
          ▼
┌─────────────────────┐
│   Tools (Registry)  │
│  - 查找工具         │
│  - 安全检查         │
│  - 执行工具         │
│  - 返回结果         │
└─────────┬───────────┘
          │
          │ Tool Result
          ▼
┌─────────────────────┐
│   Model (Continue)  │
│  - 继续对话         │
│  - 直到完成         │
└─────────────────────┘
```

---

## 安全机制

### SafetyGuard 设计

```rust
pub struct SafetyGuard {
    policy: ConfirmationPolicy,
    dangerous_patterns: Vec<Regex>,
}

pub enum ConfirmationPolicy {
    /// 始终确认
    AlwaysAsk,
    /// 白名单命令免确认
    Whitelist(Vec<String>),
    /// 只读操作免确认
    ReadOnly,
}

impl SafetyGuard {
    /// 检查命令是否需要确认
    pub fn check_command(&self, command: &str, params: &Value) -> SafetyDecision {
        // 检查危险模式
        for pattern in &self.dangerous_patterns {
            if pattern.is_match(command) {
                return SafetyDecision::RequireConfirmation(format!(
                    "危险操作检测: {}",
                    command
                ));
            }
        }
        
        // 根据策略决定
        match &self.policy {
            ConfirmationPolicy::AlwaysAsk => {
                SafetyDecision::RequireConfirmation(format!(
                    "执行命令: {}",
                    command
                ))
            }
            ConfirmationPolicy::Whitelist(allowed) => {
                if allowed.iter().any(|a| command.starts_with(a)) {
                    SafetyDecision::Allow
                } else {
                    SafetyDecision::RequireConfirmation(format!(
                        "执行命令: {}",
                        command
                    ))
                }
            }
            ConfirmationPolicy::ReadOnly => {
                if is_readonly_command(command) {
                    SafetyDecision::Allow
                } else {
                    SafetyDecision::RequireConfirmation(format!(
                        "写操作需要确认: {}",
                        command
                    ))
                }
            }
        }
    }
}

// 危险命令模式
const DANGEROUS_PATTERNS: &[&str] = &[
    r"^rm\s",           // 删除文件
    r"^sudo\s",         // 提权
    r"^chmod\s",        // 权限修改
    r"^chown\s",        // 所有者修改
    r"^dd\s",           // 磁盘操作
    r"^mkfs",           // 格式化
    r"^:\(\)\{\s*:\|\:&\s*\};:", // Fork bomb
];

// 只读命令
fn is_readonly_command(cmd: &str) -> bool {
    let readonly_prefixes = [
        "ls", "cat", "head", "tail", "less", "more",
        "grep", "rg", "find", "fd",
        "git status", "git log", "git diff", "git show",
        "cargo check", "cargo test", "cargo clippy",
    ];
    
    readonly_prefixes.iter().any(|p| cmd.starts_with(p))
}
```

### 确认交互流程

```rust
pub async fn confirm_action(prompt: &str) -> Result<bool> {
    use inquire::Confirm;
    
    let ans = Confirm::new(&format!("{}?", prompt))
        .with_default(false)
        .with_help_message("输入 y 确认，n 拒绝")
        .prompt()?;
    
    Ok(ans)
}
```

---

## 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("工具未找到: {0}")]
    ToolNotFound(String),
    
    #[error("缺少参数: {0}")]
    MissingParameter(&'static str),
    
    #[error("用户取消操作")]
    UserCancelled,
    
    #[error("模型错误: {0}")]
    Model(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

---

## 配置系统

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub model: ModelConfig,
    pub context: ContextConfig,
    pub safety: SafetyConfig,
    pub tools: ToolsConfig,
}

#[derive(Debug, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub host: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub strategy: String,
}

#[derive(Debug, Deserialize)]
pub struct SafetyConfig {
    pub policy: String,
    pub whitelist: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToolsConfig {
    pub enable_shell: bool,
    pub enable_web: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = dirs::config_dir()
            .map(|p| p.join("xflow/config.toml"));
        
        if let Some(path) = config_path {
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                return Ok(toml::from_str(&content)?);
            }
        }
        
        // 返回默认配置
        Ok(Self::default())
    }
}
```
