# xflow (心流) - AI 编程助手

> 一个类似 Claude Code 的 AI 编程开发工具，采用 Rust 实现，支持本地模型。

## 项目概述

**xflow** 是一个智能编程助手，核心特点：

- 🦀 **Rust 高性能实现** - 快速、安全、可靠
- 🏠 **本地模型支持** - 隐私优先，支持 Ollama
- 🧠 **智能上下文管理** - 自动理解项目结构
- 🛡️ **安全机制** - 交互确认，防止误操作
- 📱 **多端支持** - CLI 优先，后期支持 Web/移动端
- 🔧 **渐进式实现** - 每阶段可独立验证

## 技术栈

| 类别 | 选型 | 说明 |
|------|------|------|
| 语言 | Rust | 高性能、内存安全 |
| 异步运行时 | Tokio | 高性能异步 I/O |
| CLI 框架 | clap | 命令行参数解析 |
| 交互界面 | rustyline + inquire | REPL + 交互确认 |
| AST 解析 | tree-sitter | 多语言代码解析 |
| 代码搜索 | ripgrep (grep crate) | 快速代码搜索 |
| HTTP 客户端 | reqwest | 与 Ollama 通信 |
| 序列化 | serde / serde_json | JSON 处理 |
| Git 操作 | git2 | Git 集成 |
| 日志 | tracing | 结构化日志 |
| TUI (可选) | ratatui | 终端用户界面 |

## 核心功能

1. **代码生成/修改** - 根据自然语言生成或修改代码
2. **代码解释/分析** - 解释代码逻辑、查找问题
3. **项目理解** - 理解整个代码库结构和依赖关系
4. **Shell 命令执行** - 自动执行终端命令（带确认机制）
5. **Git 操作** - 状态查看、提交、分支管理等
6. **自动开发循环** - 多步骤任务自动执行

## 项目结构

```
xflow/
├── crates/
│   ├── xflow/              # CLI 主入口
│   ├── xflow-core/         # 核心引擎
│   ├── xflow-agent/        # Agent 系统
│   ├── xflow-context/      # 上下文管理
│   ├── xflow-tools/        # 工具系统
│   ├── xflow-model/        # 模型接口层
│   ├── xflow-lsp/          # LSP 集成 (可选)
│   └── xflow-config/       # 配置管理
├── docs/                   # 文档
├── web/                    # Web 前端 (Phase 2)
└── Cargo.toml              # Workspace 配置
```

---

## 渐进式实现路线图

### 阶段 1：基础对话 ✅ 当前目标

**目标**：CLI 能与本地模型对话

```
xflow> 你好
AI: 你好！我是 xflow 编程助手，有什么可以帮你的？
```

**实现内容**：
- [x] CLI 入口框架 (`clap`)
- [x] REPL 交互循环 (`rustyline`)
- [x] Ollama API 集成
- [x] 流式响应显示

**验证点**：能正常对话，流式输出正常

---

### 阶段 2：对话 + 文件工具

**目标**：AI 能读取文件内容

```
xflow> 帮我看看 src/main.rs 的内容
AI: [调用 read_file 工具]
    这是 src/main.rs 的内容：
    fn main() {
        println!("Hello");
    }
```

**实现内容**：
- [ ] Tool 系统框架
- [ ] `read_file` 工具实现
- [ ] Tool Use 格式处理
- [ ] 工具调用循环

**验证点**：AI 能正确读取并展示文件内容

---

### 阶段 3：对话 + 完整文件工具

**目标**：AI 能读写、搜索文件

**实现内容**：
- [ ] `write_file` 工具（带确认）
- [ ] `search_file` 工具 (grep)
- [ ] `list_directory` 工具
- [ ] 确认机制实现

**验证点**：AI 能搜索代码、修改文件（需确认）

---

### 阶段 4：对话 + Shell 工具

**目标**：AI 能执行 Shell 命令

```
xflow> 帮我运行 cargo check
AI: [请求确认] 执行命令: cargo check
你: 确认
AI: [执行结果]
    Checking xflow v0.1.0
    Finished dev target...
```

**实现内容**：
- [ ] `run_shell` 工具
- [ ] 危险命令检测
- [ ] 交互确认流程

**验证点**：AI 能执行命令，危险命令需确认

---

### 阶段 5：自动开发循环

**目标**：AI 能自动循环执行任务直到完成

```
xflow> 帮我修复所有编译错误
AI: [运行 cargo check]
    发现 3 个错误
    [读取错误文件]
    [修改代码]
    [再次检查]
    所有错误已修复！
```

**实现内容**：
- [ ] Tool Call 循环机制
- [ ] 最大循环次数限制
- [ ] 任务完成判断

**验证点**：AI 能自动完成多步骤任务

---

### 阶段 6：智能上下文管理

**目标**：AI 能智能理解项目结构

**实现内容**：
- [ ] tree-sitter AST 解析
- [ ] 项目结构扫描
- [ ] 相关文件智能选择
- [ ] Token 预算管理

**验证点**：问项目相关问题时，AI 能找到正确文件

---

### 阶段 7：Git 工具

**目标**：AI 能进行 Git 操作

**实现内容**：
- [ ] `git_status` 工具
- [ ] `git_diff` 工具
- [ ] `git_commit` 工具（带确认）
- [ ] `git_log` 工具

**验证点**：AI 能查看状态、创建提交

---

### 阶段 8：多 Agent 系统

**目标**：不同任务由专门 Agent 处理

**实现内容**：
- [ ] Agent Trait 抽象
- [ ] PlannerAgent（任务分解）
- [ ] CoderAgent（编码）
- [ ] ReviewerAgent（代码审查）
- [ ] Agent 协调器

**验证点**：复杂任务能正确分解执行

---

### 阶段 9：Web API + 前端

**目标**：提供 Web 界面

**实现内容**：
- [ ] REST/WebSocket API
- [ ] Web 前端界面
- [ ] 移动端适配

**验证点**：能通过浏览器/手机使用

---

## 验证清单

| 阶段 | 验证命令 | 预期结果 | 状态 |
|-----|---------|---------|------|
| 1 | `cargo run` → 输入对话 | 能正常对话 | ⏳ |
| 2 | 让 AI 读文件 | 能读取并展示 | - |
| 3 | 让 AI 搜索代码 | 能搜索并返回结果 | - |
| 4 | 让 AI 运行命令 | 能执行并展示结果 | - |
| 5 | 让 AI 修复错误 | 能自动完成 | - |
| 6 | 问项目问题 | 能找到相关代码 | - |
| 7 | 让 AI 提交代码 | 能创建 Git 提交 | - |
| 8 | 给复杂任务 | 能分解执行 | - |
| 9 | 打开浏览器 | 能正常使用 | - |

---

## 配置设计

### 全局配置 `~/.config/xflow/config.toml`

```toml
[model]
provider = "ollama"
host = "http://localhost:11434"
model = "codellama:7b"
# 或使用其他模型
# model = "deepseek-coder:6.7b"

[context]
max_tokens = 128000
strategy = "smart"  # smart | manual | full

[safety]
policy = "whitelist"
whitelist = ["ls", "cat", "git status", "rg", "fd"]

[tools]
enable_shell = true
enable_web = false  # 可选启用 Web 搜索
```

### 项目配置 `.xflow/config.toml`

```toml
[project]
language = "rust"
ignore = ["target/", "*.lock"]

[context]
# 项目特定配置
include = ["src/**/*.rs"]
```

---

## 开发指南

### 环境准备

1. 安装 Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. 安装 Ollama: 参考 https://ollama.ai
3. 下载模型: `ollama pull codellama:7b`

### 开发命令

```bash
# 运行开发版本
cargo run

# 运行发布版本
cargo run --release

# 运行测试
cargo test

# 代码检查
cargo clippy

# 格式化
cargo fmt
```

---

## 许可证

MIT License
