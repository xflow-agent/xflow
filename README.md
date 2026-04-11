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

### 🚧 阶段 5：自动开发循环（待实现）

**目标**: AI 能自动循环执行任务直到完成

---

### 🚧 阶段 6：智能上下文管理（待实现）

**目标**: AI 能智能理解项目结构

---

### 🚧 阶段 7：Git 工具（待实现）

**目标**: AI 能进行 Git 操作

---

### 🚧 阶段 8：多 Agent 系统（待实现）

**目标**: 不同任务由专门 Agent 处理

---

### 🚧 阶段 9：Web API + 前端（待实现）

**目标**: 提供 Web 界面

## 项目结构

```
xflow/
├── crates/
│   ├── xflow/              # CLI 主入口
│   ├── xflow-core/         # 核心引擎 (Session, Agent Loop)
│   ├── xflow-model/        # 模型接口层 (Ollama)
│   └── xflow-tools/        # 工具系统
│       ├── read_file.rs    # 读取文件
│       ├── write_file.rs   # 写入文件
│       ├── list_directory.rs # 列出目录
│       ├── search_file.rs  # 搜索代码
│       └── run_shell.rs    # 执行 Shell 命令
├── docs/                   # 设计文档
│   ├── PLAN.md             # 实施计划
│   └── ARCHITECTURE.md     # 架构设计
└── Cargo.toml              # Workspace 配置
```

## 内置工具

| 工具 | 功能 | 需确认 |
|------|------|--------|
| `read_file` | 读取文件内容 | ❌ |
| `write_file` | 写入文件（覆盖） | ✅ |
| `list_directory` | 列出目录内容 | ❌ |
| `search_file` | 搜索代码 (ripgrep) | ❌ |
| `run_shell` | 执行 Shell 命令 | ✅ |

## REPL 命令

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助 |
| `/exit` | 退出程序 |
| `/clear` | 清空对话历史 |

## 开发命令

```bash
# 运行开发版本
cargo run

# 运行发布版本
cargo run --release

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
