# xflow (心流) - AI 编程助手开发计划 v2

> 基于2026年4月项目审计和Claude Code最佳实践分析制定

---

## 一、项目审计摘要

### 1.1 已实现功能清单

| 模块 | 功能 | 完成度 | 文件位置 |
|------|------|--------|----------|
| **CLI (xflow)** | REPL交互、历史记录、命令处理 | 100% | `crates/xflow/src/` |
| **模型层 (xflow-model)** | Ollama集成、流式响应、工具调用 | 100% | `crates/xflow-model/src/` |
| **工具系统 (xflow-tools)** | read/write/list/search/shell/git | 100% | `crates/xflow-tools/src/` |
| **安全机制** | 危险命令检测(3级)、确认机制 | 100% | `crates/xflow-tools/src/run_shell.rs` |
| **Agent系统 (xflow-agent)** | ReviewerAgent, CoderAgent, Agent作为工具 | 100% | `crates/xflow-agent/src/` |
| **核心引擎 (xflow-core)** | 工具调用循环(MAX 20)、输出回调 | 100% | `crates/xflow-core/src/` |
| **上下文管理 (xflow-context)** | 项目扫描、语言检测(30+)、Token估算 | 100% | `crates/xflow-context/src/` |
| **Web服务 (xflow-server)** | REST API、WebSocket、前端界面 | 100% | `crates/xflow-server/src/` |

### 1.2 架构评价

**优点**：
- 模块化设计清晰，6个crate职责分明
- Rust最佳实践：async_trait, thiserror, tracing
- 危险命令3级检测设计优秀
- Agent作为Tool的设计实现了主流程调用Agent的能力
- UTF-8安全截断使用char_indices()

**不足**：
- 仅支持Ollama，未实现其他模型后端
- 缺少LSP集成和AST解析(tree-sitter未实现)
- 无会话持久化和配置文件系统
- Web端自动确认危险操作存在安全隐患
- 无上下文压缩机制

---

## 二、Claude Code最佳工程实践

### 2.1 工具系统设计

| 实践 | Claude Code实现 | 借鉴价值 |
|------|----------------|----------|
| 工具隔离 | 每个工具独立权限门控 | 高 |
| 延迟加载 | 工具按需加载，防止上下文膨胀 | 中 |
| Agent作为工具 | 子Agent是工具注册表的一等公民 | 已实现 |
| 专用搜索工具 | 独立的Grep和Glob工具 | 已实现 |

### 2.2 三层上下文压缩（核心借鉴）

```
┌─────────────────────────────────────────────────────────────┐
│ MicroCompact                                                │
│ - 触发: 随时                                                 │
│ - 实现: 本地修剪旧工具输出                                    │
│ - 成本: 零API调用                                            │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│ AutoCompact                                                 │
│ - 触发: 接近上下文窗口上限                                    │
│ - 实现: 生成结构化摘要，预留13K Token缓冲                     │
│ - 熔断: 3次连续失败后停止                                     │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│ FullCompact                                                 │
│ - 触发: 严重溢出                                             │
│ - 实现: 全量压缩 + 选择性文件重新注入(5K/文件)                │
│ - 重置: 工作预算恢复到50K Token                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 三层记忆索引

```
Index (始终加载)     → ~150字符/行，仅指针
Topic Files (按需)   → 实际知识内容
Transcripts (grep)   → 从不直接加载
```

**写入纪律**：先写topic file，再更新index，从不向index直接dump内容。

### 2.4 Prompt Cache优化

Claude Code使用`SYSTEM_PROMPT_DYNAMIC_BOUNDARY`分割：

```
[静态部分: 身份、工具定义、环境上下文] → 全局缓存，节省~70% Token
──────────── SYSTEM_PROMPT_DYNAMIC_BOUNDARY ────────────
[动态部分: CLAUDE.md、git状态、当前日期] → 会话特定
```

### 2.5 Mailbox审批模式

```
Worker Agent ──请求──► Coordinator邮箱 ──等待──► 审批结果
                              │
                              ├── 批准 → 执行
                              └── 拒绝 → 取消
```

**关键设计**：原子声明机制防止两个Worker同时处理同一审批。

---

## 三、差距分析与优先级

### 3.1 P0 紧急且重要

| 差距 | 影响 | 解决方案 |
|------|------|----------|
| Web端auto_confirm | 安全风险 | 实现确认对话框UI |
| 无配置系统 | 用户无法自定义 | `~/.config/xflow/config.toml` |
| 无上下文压缩 | 长会话失败 | 三层压缩实现 |

### 3.2 P1 重要不紧急

| 差距 | 影响 | 解决方案 |
|------|------|----------|
| 会话不持久化 | 重启丢失 | 本地存储(JSONL) |
| 仅支持Ollama | 灵活性受限 | OpenAI API适配层 |
| 无LSP集成 | 代码理解弱 | LSP client实现 |
| 无AST解析 | 无法精确分析 | tree-sitter集成 |

### 3.3 P2 可延后

| 差距 | 影响 | 解决方案 |
|------|------|----------|
| 无Prompt Cache | Token成本高 | 动态边界分割 |
| 无多Agent编排 | 并行能力弱 | Mailbox/Fork模式 |
| 无遥测 | 无使用洞察 | 挫折指标+熔断器 |

---

## 四、开发路线图

### Sprint 1: 安全与配置 (P0)

**周期**: Week 1-2

#### A1. Web端确认对话框

**目标**: 解决`auto_confirm=true`安全隐患

**前端实现** (`web/app.js`):
```javascript
// 确认弹窗组件
function showConfirmationDialog(request) {
    return new Promise((resolve) => {
        const dialog = document.createElement('div');
        dialog.className = 'confirmation-dialog';
        dialog.innerHTML = `
            <div class="dialog-content">
                <h3>⚠️ 需要确认</h3>
                <p>${request.message}</p>
                <div class="dialog-actions">
                    <button id="confirm-yes">确认执行</button>
                    <button id="confirm-no">取消</button>
                </div>
            </div>
        `;
        // ...
    });
}
```

**WebSocket协议扩展**:
```json
// 请求
{
    "type": "confirmation_request",
    "id": "uuid",
    "tool": "run_shell",
    "message": "执行命令: rm -rf target/"
}

// 响应
{
    "type": "confirmation_response",
    "id": "uuid",
    "approved": true
}
```

**后端修改** (`crates/xflow-server/src/ws.rs`):
- 移除`auto_confirm = true`默认值
- 添加确认请求发送和响应等待逻辑

#### A2. 配置文件系统

**新建crate**: `crates/xflow-config/`

**配置结构**:
```toml
# ~/.config/xflow/config.toml

[model]
provider = "ollama"           # ollama | openai | anthropic
host = "http://localhost:11434"
model = "codellama:7b"

[context]
max_tokens = 128000
strategy = "smart"            # smart | manual | full
compression_threshold = 0.8   # 触发压缩的阈值

[safety]
policy = "ask"                # always_ask | whitelist | read_only
danger_level = 2              # 1-3，危险命令检测级别

[tools]
enable_shell = true
enable_git = true
```

**实现要点**:
- 使用`dirs`库定位配置目录
- 使用`toml`解析配置
- 提供默认配置fallback

#### A3. 会话持久化

**存储格式**: JSONL (每行一个JSON对象)

**存储位置**: `~/.local/share/xflow/sessions/{session_id}.jsonl`

**数据结构**:
```rust
struct SessionRecord {
    timestamp: DateTime<Utc>,
    role: Role,
    content: String,
    tool_calls: Option<Vec<ToolCallRecord>>,
}
```

**清理策略**: 保留最近30天，最多100个会话

---

### Sprint 2: 上下文管理增强 (P0-P1)

**周期**: Week 3-4

#### B1. 三层压缩实现

**新建模块**: `crates/xflow-core/src/compact.rs`

```rust
pub enum CompactLevel {
    Micro,   // 本地修剪
    Auto,    // 摘要生成
    Full,    // 全量压缩
}

pub struct CompactConfig {
    pub auto_trigger_threshold: f32,  // 0.8 = 80%
    pub reserved_buffer: usize,        // 13000 tokens
    pub max_consecutive_failures: u8,  // 3
}
```

**压缩流程**:
1. 检查当前Token使用率
2. 超过阈值触发AutoCompact
3. 调用模型生成摘要
4. 替换旧消息为摘要
5. 失败计数，达到熔断值停止

#### B2. 记忆系统

**新建模块**: `crates/xflow-context/src/memory.rs`

```rust
pub struct MemorySystem {
    index: MemoryIndex,           // 始终加载
    topic_files: TopicFileStore,  // 按需加载
    transcripts: TranscriptStore, // 仅grep
}

impl MemorySystem {
    pub fn query(&self, topic: &str) -> Vec<KnowledgeEntry> {
        // 1. 从index查找指针
        // 2. 按需加载topic file
        // 3. 返回知识条目
    }
    
    pub fn write(&mut self, entry: KnowledgeEntry) {
        // 1. 先写入topic file
        // 2. 再更新index
        // 3. 不向index直接dump
    }
}
```

#### B3. Token预算管理

```rust
pub struct TokenBudget {
    max_tokens: usize,
    current_usage: usize,
    reserved: usize,
}

impl TokenBudget {
    pub fn can_add(&self, tokens: usize) -> bool {
        self.current_usage + tokens + self.reserved < self.max_tokens
    }
    
    pub fn compression_needed(&self) -> bool {
        self.current_usage as f32 / self.max_tokens as f32 > 0.8
    }
}
```

---

### Sprint 3: 模型与代码理解 (P1)

**周期**: Week 5-6

#### C1. 多模型后端

**扩展**: `crates/xflow-model/src/openai.rs`

```rust
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn chat_stream_with_tools(&self, ...) { ... }
}
```

**配置切换**:
```rust
fn create_provider(config: &ModelConfig) -> Box<dyn ModelProvider> {
    match config.provider.as_str() {
        "ollama" => Box::new(OllamaProvider::new(config)),
        "openai" => Box::new(OpenAIProvider::new(config)),
        _ => panic!("Unknown provider"),
    }
}
```

#### C2. AST解析 (tree-sitter)

**新建crate**: `crates/xflow-ast/`

**依赖**:
```toml
[dependencies]
tree-sitter = "0.20"
tree-sitter-rust = "0.20"
tree-sitter-python = "0.20"
tree-sitter-javascript = "0.20"
```

**核心功能**:
```rust
pub struct AstParser {
    parsers: HashMap<Language, Parser>,
}

impl AstParser {
    pub fn parse(&self, code: &str, lang: Language) -> Tree;
    pub fn extract_symbols(&self, tree: &Tree) -> Vec<Symbol>;
    pub fn find_references(&self, tree: &Tree, symbol: &str) -> Vec<Location>;
}
```

#### C3. LSP集成

**新建crate**: `crates/xflow-lsp/`

**支持能力**:
- 代码补全 (completion)
- 跳转定义 (goto_definition)
- 查找引用 (find_references)
- 重命名 (rename)

---

### Sprint 4: 高级特性 (P2)

**周期**: Week 7-8

#### D1. Prompt Cache优化

**实现位置**: `crates/xflow-core/src/prompt.rs`

```rust
pub struct PromptBuilder {
    static_prefix: String,      // 缓存
    dynamic_boundary: String,   // 标记
    dynamic_suffix: String,     // 每次重建
}

impl PromptBuilder {
    pub fn build(&self) -> String {
        format!(
            "{}\n{}\n{}",
            self.static_prefix,
            self.dynamic_boundary,
            self.dynamic_suffix
        )
    }
}
```

#### D2. 多Agent编排

**Mailbox模式**:
```rust
pub struct Mailbox {
    pending_requests: VecDeque<ApprovalRequest>,
    tx: mpsc::Sender<ApprovalResponse>,
}

pub struct WorkerAgent {
    mailbox_tx: mpsc::Sender<ApprovalRequest>,
}
```

**Fork模式**:
```rust
pub fn fork_agent(parent_ctx: &AgentContext) -> AgentContext {
    // 字节级复制，共享Prompt Cache
    parent_ctx.clone()
}
```

---

## 五、验证清单

| Sprint | 验证命令 | 预期结果 |
|--------|---------|---------|
| 1-A1 | Web端执行`rm`命令 | 弹出确认对话框 |
| 1-A2 | 启动CLI，检查配置加载 | 使用config.toml配置 |
| 1-A3 | 重启服务，检查会话 | 会话历史保留 |
| 2-B1 | 长对话超过阈值 | 自动压缩 |
| 2-B2 | 添加记忆，重启 | 记忆保留 |
| 3-C1 | 配置切换到OpenAI | 使用OpenAI模型 |
| 3-C2 | 询问代码符号 | 返回精确符号信息 |

---

## 六、技术债务清单

| 债务 | 位置 | 建议 |
|------|------|------|
| 未使用字段 `workdir` | read_file.rs, write_file.rs | 移除或实现功能 |
| 硬编码 MAX_TOOL_LOOPS | session.rs:18 | 移到配置文件 |
| 重复的系统提示词 | ollama.rs, session.rs | 统一到一处 |
| Web端CORS配置宽松 | server/main.rs | 限制允许的来源 |

---

## 七、参考资料

- Claude Code源码泄露分析 (2026年3月)
- Anthropic Agent SDK文档
- tree-sitter官方文档
- LSP协议规范

---

**文档版本**: v2.0
**更新日期**: 2026-04-12
**基于**: 项目全面审计 + Claude Code最佳实践分析
