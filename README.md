# xflow (心流) - AI 编程助手

> 一个类似 Claude Code 的 AI 编程开发工具，采用 Rust 实现，支持本地模型。

## 项目目标

**xflow** 致力于打造一个智能、安全、高效的 AI 编程助手：

- 🦀 **Rust 高性能实现** - 快速、安全、可靠
- 🏠 **本地模型支持** - 隐私优先，支持 Ollama
- 🧠 **智能上下文管理** - 自动理解项目结构
- 🛡️ **安全机制** - 危险操作需交互确认
- 🔧 **渐进式实现** - 每阶段可独立验证

## 快速开始

### 环境准备

1. 安装 Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. 安装 Ollama: 参考 https://ollama.ai
3. 下载模型: `ollama pull gemma4:e4b`

### 运行

```bash
# 编译
cargo build --release

# 运行
./target/release/xflow

# 或指定模型
./target/release/xflow --model gemma4:e4b
```

## 渐进式实现路线图

### ✅ 阶段 1：CLI 基础对话功能

**目标**: CLI 能与本地模型对话

**实现内容**:
- CLI 入口框架 (clap)
- REPL 交互循环 (rustyline)
- Ollama API 集成
- 流式响应显示

**验证测试**:
```
xflow> 你好，介绍一下你自己
AI: 你好！我是 xflow 编程助手...
```

---

### ✅ 阶段 2：工具系统 + read_file

**目标**: AI 能读取文件内容

**实现内容**:
- Tool trait 定义
- ToolRegistry 工具注册表
- read_file 工具实现
- Agent Loop 工具调用循环
- 对接 Ollama tool calling API

**验证测试**:
```
xflow> 请读取 Cargo.toml 文件的内容
[调用工具: read_file]
[工具结果: 1109 字节]
AI: 我已经读取了 Cargo.toml 文件，这是一个 Cargo 工作区配置...
```

---

### ✅ 阶段 3：完整文件工具 + 确认机制

**目标**: AI 能读写、搜索文件，危险操作需确认

**实现内容**:
- write_file 工具（带确认）
- list_directory 工具
- search_file 工具 (ripgrep)
- 交互式确认机制 (inquire)

**验证测试**:
```
# 列出目录
xflow> 列出 crates 目录的内容
[调用工具: list_directory]
AI: crates 目录包含: xflow/, xflow-core/, xflow-model/, xflow-tools/

# 搜索代码
xflow> 搜索代码中的 async fn 关键字
[调用工具: search_file]
AI: 找到 27 处匹配...

# 写入文件（需确认）
xflow> 创建测试文件 test.txt，内容是 Hello
[调用工具: write_file]
==================================================
⚠️  需要确认操作
==================================================
工具: write_file
路径: "test.txt"
内容预览: Hello
==================================================
是否执行此操作? [y/N]
```

---

### ✅ 阶段 4：Shell 工具 + 危险命令检测

**目标**: AI 能执行 Shell 命令，危险命令需确认

**实现内容**:
- run_shell 工具（带超时）
- 三级危险命令检测
- 交互确认流程

**危险等级**:

| 等级 | 图标 | 示例 | 说明 |
|------|------|------|------|
| 3 | 🔴 | `rm -rf /`, `mkfs`, `curl ... \| bash` | 极度危险 |
| 2 | 🟠 | `rm -rf`, `shutdown`, `killall` | 高度危险 |
| 1 | 🟡 | `rm`, `chmod`, `sudo`, `curl` | 中度危险 |

**验证测试**:
```
# 安全命令
xflow> 执行 ls -la 命令
[调用工具: run_shell]
==================================================
⚠️  需要确认操作
==================================================
工具: run_shell
命令: ls -la
==================================================
是否执行此操作? [y/N] y
[工具结果: ...]
AI: 当前目录包含...

# 危险命令
xflow> 删除所有 node_modules 目录: rm -rf node_modules
[调用工具: run_shell]
==================================================
🟠 高度危险 - 递归强制删除
==================================================
工具: run_shell
命令: rm -rf node_modules
==================================================
⚠️  确认执行此危险操作? [y/N]
```

---

### ✅ 阶段 5：自动开发循环

**目标**: AI 能自动循环执行任务直到完成

**实现内容**:
- 系统提示词（定义 AI 角色和行为准则）
- 循环进度显示
- 任务完成统计
- 最大循环次数限制 (20 次)

**验证测试**:

单步任务：
```
xflow> 请读取 Cargo.toml，然后告诉我项目名称

[调用工具: read_file]
[结果: 1109 字节]

── 自动执行 (第 2/20 轮) ──
根据文件内容，项目名称是：xflow

✅ 任务完成 (共调用 1 次工具, 1 轮循环)
```

多步骤任务（自动循环执行）：
```
xflow> 在 /tmp/test_xflow 目录创建一个 Rust 项目，包含 main.rs 打印 Hello World，然后编译运行

[调用工具: run_shell] mkdir -p /tmp/test_xflow
[结果: ...]

── 自动执行 (第 2/20 轮) ──
[调用工具: run_shell] cd /tmp/test_xflow && cargo init
[结果: ...]

── 自动执行 (第 3/20 轮) ──
[调用工具: write_file] 写入 src/main.rs
==================================================
⚠️  需要确认操作
==================================================
路径: /tmp/test_xflow/src/main.rs
内容预览: fn main() { println!("Hello World"); }
==================================================
是否执行此操作? [y/N] y
✓ 已确认，执行操作...
[结果: 成功写入文件]

── 自动执行 (第 4/20 轮) ──
[调用工具: run_shell] cd /tmp/test_xflow && cargo run
[结果: Hello World]

✅ 任务完成 (共调用 4 次工具, 4 轮循环)
```

**系统提示词要点**:
- 完整执行：多步骤任务必须执行所有步骤
- 自动循环：不要中途停止
- 及时汇报：让用户了解进度
- 安全意识：危险操作需确认

---

### ✅ 阶段 6：智能上下文管理

**目标**: AI 能智能理解项目结构

**实现内容**:
- 项目结构扫描器 (ProjectScanner)
- 语言检测和文件类型识别
- Token 估算器
- 上下文构建器 (ContextBuilder)
- 启动时自动扫描并注入项目上下文

**验证测试**:
```bash
./target/release/xflow

# 启动时会自动扫描项目并显示：
╔═════════════════════════════════════════╗
║           xflow - 心流编程助手           ║
╚═════════════════════════════════════════╝
📁 正在扫描项目目录...
   xflow (Rust) - 38 文件, 23 源文件, 语言: Rust

xflow> 这个项目的结构是什么？
AI: 根据扫描结果，这是一个 Rust 工作区项目...

xflow> 主要的源文件有哪些？
AI: [调用 list_directory 工具]
    根据 Cargo.toml 配置...
```

**实现特性**:
- 自动识别项目类型 (Rust/Node/Python/Go/Java 等)
- 统计各语言文件数量
- 生成简化的目录树
- 识别重要文件 (Cargo.toml, main.rs 等)
- Token 预算管理

---

### ✅ 阶段 7：Git 工具

**目标**: AI 能进行 Git 操作

**实现内容**:
- git_status 工具 - 查看仓库状态
- git_diff 工具 - 查看文件差异
- git_log 工具 - 查看提交历史
- git_commit 工具 - 创建提交（需确认）
- git_add 工具 - 添加文件到暂存区
- git_branch 工具 - 管理分支

**验证测试**:
```bash
./target/release/xflow

# 查看状态
xflow> 查看 git 状态
[调用工具: git_status]
AI: 当前分支: main
    你的分支与上游分支一致...

# 查看差异
xflow> 查看 src/main.rs 的更改
[调用工具: git_diff]
AI: diff --git a/src/main.rs b/src/main.rs...

# 查看日志
xflow> 查看最近 5 次提交
[调用工具: git_log]
AI: 6c9db73 feat: 阶段6 - 智能上下文管理
    2701a61 docs: 更新 README...

# 创建提交（需确认）
xflow> 提交这些更改，消息是 "fix: 修复问题"
[调用工具: git_commit]
==================================================
⚠️  需要确认操作
==================================================
工具: git_commit
提交消息: "fix: 修复问题"
==================================================
是否执行此操作? [y/N] y
✓ 已确认，执行操作...
AI: 提交成功！新提交: abc1234...
```

---

### ✅ 阶段 8：Agent 工具集成

**目标**: 将 Agent 作为高级工具集成到主对话

**实现内容**:
- Agent trait 抽象接口
- ReviewerAgent - 代码审查/分析（作为 `analyze_project` 工具）
- CoderAgent - 代码编写（作为 `implement_feature` 工具）
- Agent 内部工具循环（只调用基础工具）

**Agent 工具**:

| 工具 | 功能 | 触发条件 |
|------|------|---------|
| `analyze_project` | 项目分析 | "分析项目"、"分析功能" |
| `implement_feature` | 功能实现 | "实现xxx"、"添加xxx功能" |

**架构设计**:
```
用户请求
    │
    ▼
┌─────────────────────┐
│   主对话 AI         │
│  (Session)          │
└──────────┬──────────┘
           │
     ┌─────┼─────┐
     ▼     ▼     ▼
┌─────┐ ┌─────┐ ┌─────┐
│基础 │ │Agent│ │直接 │
│工具 │ │工具 │ │回复 │
└─────┘ └──┬──┘ └─────┘
           │
           ▼
      Agent 内部循环
           │
           ▼
      基础工具调用
   (read_file, run_shell...)
```

**验证测试**:
```
xflow> 分析一下这个项目的所有功能

[调用工具: analyze_project]

📊 开始项目分析...

[Agent 调用工具: read_file] Cargo.toml
[结果: 1259 字节]
[Agent 调用工具: read_file] README.md
[结果: 13568 字节]
...

✅ 分析完成
AI: 这是一个 Rust 工作区项目，包含以下模块...
```

**关键规则**:
- 主对话可以调用：基础工具 + Agent 工具
- Agent 只能调用：基础工具（不能调用其它 Agent）
- Agent 有自己的工具循环

---

### ✅ 阶段 9：Web API + 前端

**目标**: 提供 Web 界面

**实现内容**:
- REST API 服务器 (axum)
- WebSocket 实时通信
- Web 前端界面 (HTML/CSS/JS)
- 会话管理

**API 端点**:

| 端点 | 方法 | 功能 |
|------|------|------|
| `/api/sessions` | POST | 创建新会话 |
| `/api/sessions` | GET | 列出所有会话 |
| `/api/sessions/:id` | GET | 获取会话信息 |
| `/api/sessions/:id/chat` | POST | 发送消息 |
| `/api/sessions/:id/clear` | POST | 清空会话 |
| `/api/sessions/:id` | DELETE | 删除会话 |
| `/api/ws` | WebSocket | 实时通信 |
| `/health` | GET | 健康检查 |

**启动命令**:
```bash
# 启动 Web 服务器
cargo run --bin xflow-server

# 指定参数
cargo run --bin xflow-server -- --addr 0.0.0.0:3000 --model qwen2.5:7b
```

**验证测试**:
打开浏览器访问 `http://localhost:3000`，可正常对话。

---

## 项目结构

```
xflow/
├── crates/
│   ├── xflow/              # CLI 主入口
│   ├── xflow-core/         # 核心引擎 (Session, Agent Loop)
│   ├── xflow-model/        # 模型接口层 (Ollama)
│   ├── xflow-context/      # 上下文管理
│   ├── xflow-agent/        # Agent 系统
│   │   ├── agent.rs        # Agent trait 抽象
│   │   ├── coder.rs        # 代码编写器
│   │   └── reviewer.rs     # 代码审查器
│   ├── xflow-server/       # Web API 服务器
│   │   ├── api.rs          # REST API
│   │   ├── ws.rs           # WebSocket
│   │   └── state.rs        # 状态管理
│   └── xflow-tools/        # 工具系统
│       ├── read_file.rs    # 读取文件
│       ├── write_file.rs   # 写入文件
│       ├── list_directory.rs # 列出目录
│       ├── search_file.rs  # 搜索代码
│       ├── run_shell.rs    # 执行 Shell 命令
│       ├── git.rs          # Git 操作工具
│       └── agent_tool.rs   # Agent 工具包装
├── web/                    # Web 前端
│   ├── index.html          # 主页面
│   ├── styles.css          # 样式
│   └── app.js              # 逻辑
├── docs/                   # 设计文档
│   ├── PLAN.md             # 实施计划
│   └── ARCHITECTURE.md     # 架构设计
└── Cargo.toml              # Workspace 配置
```

## 内置工具

### 文件工具

| 工具 | 功能 | 需确认 |
|------|------|--------|
| `read_file` | 读取文件内容 | ❌ |
| `write_file` | 写入文件（覆盖） | ✅ |
| `list_directory` | 列出目录内容 | ❌ |
| `search_file` | 搜索代码 (ripgrep) | ❌ |

### Shell 工具

| 工具 | 功能 | 需确认 |
|------|------|--------|
| `run_shell` | 执行 Shell 命令 | ✅ |

### Git 工具

| 工具 | 功能 | 需确认 |
|------|------|--------|
| `git_status` | 查看仓库状态 | ❌ |
| `git_diff` | 查看文件差异 | ❌ |
| `git_log` | 查看提交历史 | ❌ |
| `git_add` | 添加文件到暂存区 | ❌ |
| `git_commit` | 创建提交 | ✅ |
| `git_branch` | 管理分支 | ❌ |

### Agent 工具（高级）

| 工具 | 功能 | 触发条件 |
|------|------|---------|
| `analyze_project` | 项目分析（多步骤） | "分析项目"、"分析功能" |
| `implement_feature` | 功能实现（多步骤） | "实现xxx"、"添加xxx功能" |

## REPL 命令

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助 |
| `/exit` | 退出程序 |
| `/clear` | 清空对话历史 |

## 开发命令

```bash
# 运行 CLI 版本
cargo run

# 运行发布版本
cargo run --release

# 运行 Web 服务器
cargo run --bin xflow-server

# 运行 Web 服务器（指定参数）
cargo run --bin xflow-server -- --addr 0.0.0.0:3000 --model qwen2.5:7b

# 代码检查
cargo clippy

# 格式化
cargo fmt
```

## 配置

### 默认配置

- 模型: `gemma4:e4b`
- Ollama 地址: `http://localhost:11434`
- 工作目录: 当前目录

### 命令行参数

```bash
xflow [OPTIONS]

OPTIONS:
    -m, --model <MODEL>      模型名称 [default: gemma4:e4b]
    --host <HOST>            Ollama 地址 [default: http://localhost:11434]
    -d, --workdir <WORKDIR>  工作目录 [default: .]
    --debug                  启用调试模式
```

## 许可证

MIT License
