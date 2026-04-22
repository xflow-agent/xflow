# xflow - AI 编程助手

Rust 实现的 AI 编程开发工具，支持本地模型（Ollama/vLLM）。

## 特性

- **本地模型支持** - 隐私优先，支持 Ollama/vLLM
- **工具调用** - 文件操作、Shell 执行、Git 操作、代码搜索
- **安全确认** - 危险操作需用户确认
- **自动循环** - 多步骤任务自动执行直至完成
- **Web 界面** - 提供浏览器交互界面

## 快速开始

### 安装

```bash
# 克隆项目
git clone https://github.com/xflow/xflow
cd xflow

# 编译
cargo build --release
```

### 运行

**CLI 版本:**
```bash
# 默认使用本地 Ollama
./target/release/xflow

# 指定模型
./target/release/xflow --model qwen2.5:7b

# 指定 vLLM 服务器
./target/release/xflow --base-url http://localhost:8000/v1
```

**Web 版本:**
```bash
./target/release/xflow-server

# 浏览器访问 http://localhost:3000
```

## 使用示例

```
xflow> 读取 Cargo.toml 内容
xflow> 搜索所有 async fn 定义
xflow> 执行 cargo build
xflow> 分析项目结构
```

## 工具列表

| 工具 | 功能 | 需确认 |
|------|------|--------|
| `read_file` | 读取文件 | - |
| `write_file` | 写入文件 | ✅ |
| `list_directory` | 列出目录 | - |
| `search_file` | 代码搜索 | - |
| `run_shell` | 执行命令 | ✅ |
| `git_status/diff/log/commit` | Git 操作 | 部分 ✅ |

## 架构设计

### 核心架构

```
┌─────────────────────────────────────────────────────────────┐
│                         Session                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Message     │  │ ToolRegistry│  │ UiAdapter           │  │
│  │ History     │  │ (动态工具)   │  │ (CLI/Web/Headless)  │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌─────────────────┐    ┌──────────────┐
│  CliAdapter   │    │ WebSocketAdapter│    │AutoConfirm   │
│  (终端交互)    │    │ (Web实时通信)    │    │ (自动确认)    │
└───────────────┘    └─────────────────┘    └──────────────┘
```

### 事件流

```
Model ──► Session ──► XflowEvent ──► UiAdapter ──► UI 展示
              │
              ▼
        ToolRegistry
              │
              ▼
        Tool::execute()
```

### 添加新工具

在 `crates/xflow-tools/src/` 下创建新文件，实现 `Tool` trait:

```rust
use async_trait::async_trait;
use serde_json::Value;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "工具描述" }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "my_tool",
            description: "工具描述",
            category: ToolCategory::File,  // 选择类别
            requires_confirmation: false,   // 是否需要确认
            danger_level: 0,                // 危险等级 0-3
            display: ToolDisplayConfig {
                primary_param: "path",      // 主要展示参数
                result_display: ResultDisplayType::Summary,
                ..Default::default()
            },
        }
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        // 实现工具逻辑
        Ok("结果".to_string())
    }
}
```

然后在 `crates/xflow-tools/src/lib.rs` 注册:

```rust
registry.register(Arc::new(MyTool));
```

### 项目结构

```
crates/
├── xflow/          # 主命令行入口
├── xflow-agent/    # 代理核心逻辑
├── xflow-context/  # 上下文管理
├── xflow-core/     # 核心功能
├── xflow-model/    # 模型交互
├── xflow-server/   # 服务器
└── xflow-tools/    # 工具集
web/                # Web 界面
```

## 命令行参数

```
OPTIONS:
    -m, --model <MODEL>      模型名称 [default: gemma4:e4b]
    --base-url <BASE_URL>    API 地址 [default: http://localhost:11434/v1]
    -d, --workdir <WORKDIR>  工作目录 [default: .]
```

## 开发

```bash
cargo run           # 运行 CLI
cargo run --bin xflow-server  # 运行 Web 服务器
cargo clippy        # 代码检查
cargo fmt           # 格式化
```

## License

MIT
