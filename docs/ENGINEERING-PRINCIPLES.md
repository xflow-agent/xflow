# xflow 工程原则

> 本文档总结了项目开发的核心原则和决策方法论，旨在避免"补丁式开发"，确保所有实现都向生产级系统目标前进。

---

## 一、核心原则

### 1.1 禁止的行为

| 行为 | 问题 | 正确做法 |
|------|------|----------|
| **反复修改方案** | 浪费时间，说明思考不充分 | 先充分调研，一次性设计完整方案 |
| **选择"更简单"的方案** | 回避核心问题，技术债务累积 | 找到最佳工程实践，不管多复杂都要实现 |
| **补丁式修复** | 问题蔓延，架构腐化 | 系统性设计，分层抽象 |
| **糊弄的方案** | 无法进入生产环境 | 每个实现都要达到生产级质量 |

### 1.2 必须遵守的流程

```
┌─────────────────────────────────────────────────────────────────────┐
│  Step 1: 联网搜索最佳工程实践                                         │
│    - 搜索关键词：architecture, best practice, pattern, production   │
│    - 阅读多个来源，交叉验证                                           │
│    - 特别关注：Rust 官方推荐、知名开源项目实现                         │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Step 2: 深入理解问题本质                                             │
│    - 识别核心矛盾（如：同步回调 vs 异步操作）                         │
│    - 分析约束条件（如：不能 block async runtime）                     │
│    - 列出所有可能的解决方案                                           │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Step 3: 选择最佳方案（不是最简单方案）                                │
│    - 评估每个方案的优缺点                                             │
│    - 考虑扩展性和维护性                                               │
│    - 参考业界成熟实践                                                 │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Step 4: 拆分成小步骤实现                                             │
│    - 每个步骤独立可测试                                               │
│    - 增量提交，保持代码可编译                                         │
│    - 记录每个步骤的设计意图                                           │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 二、架构设计原则

### 2.1 Ports & Adapters Architecture（六边形架构）

**核心思想**：业务逻辑与基础设施解耦

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Application Core                                 │
│                    （业务逻辑，UI 无感知）                            │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Port (trait/interface)                    │   │
│  │  - 定义抽象接口，不依赖任何具体实现                           │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
                              ↑
                    实现 Port 接口
                              │
┌─────────────────────────────────────────────────────────────────────┐
│                     Adapters (适配器)                                │
│                                                                      │
│  ┌─────────────────┐    ┌─────────────────┐                        │
│  │  CLI Adapter    │    │  Web Adapter    │    ...                 │
│  └─────────────────┘    └─────────────────┘                        │
└─────────────────────────────────────────────────────────────────────┘
```

**应用案例**：Interaction trait

```rust
// Port: 抽象接口
#[async_trait]
pub trait Interaction: Send + Sync {
    async fn request_confirmation(&self, req: ConfirmationRequest) -> ConfirmationResult;
    fn check_interrupt(&self) -> bool;
    fn create_child_context(&self, name: &str) -> Box<dyn Interaction>;
}

// Adapter: CLI 实现
pub struct CliInteraction { ... }
impl Interaction for CliInteraction { ... }

// Adapter: WebSocket 实现
pub struct WebSocketInteraction { ... }
impl Interaction for WebSocketInteraction { ... }
```

### 2.2 分层设计原则

**OSI 模型思想**：每层只依赖直接下层，不跨层依赖

```
Layer 6: UI 层          → 只依赖 Layer 5
Layer 5: Session 层     → 只依赖 Layer 4
Layer 4: Agent 层       → 只依赖 Layer 3
Layer 3: Tool 层        → 只依赖 Layer 2
Layer 2: Model 层       → 只依赖 Layer 1
Layer 1: Infrastructure → 无依赖
```

**好处**：
- 可以独立替换任何一层
- 测试时可以 mock 任何一层
- 新增 UI 只需实现对应 Adapter

---

## 三、Rust 异步编程原则

### 3.1 Sync 与 Async 交互

**核心问题**：同步代码中如何等待异步结果？

**最佳实践**：使用 Channel

```rust
// ❌ 错误：在同步回调中无法 .await
fn sync_callback() -> bool {
    let result = async_operation().await; // 编译错误！
    result
}

// ✅ 正确：使用 oneshot channel
fn sync_callback_with_channel(tx: Sender<bool>) {
    // 发送请求
    tx.send(request);
    // 同步等待响应（通过 std::sync::mpsc 或 crossbeam）
    rx.recv().unwrap()
}
```

**参考来源**：
- [Rust Forum: Communicating between sync and async code](https://users.rust-lang.org/t/communicating-between-sync-and-async-code/41005)
- "Channels are indeed the preferred way to communicate between sync and async code."

### 3.2 避免 Blocking Async Runtime

**规则**：
- ❌ 永远不要在 async 函数中调用 `std::thread::sleep`
- ❌ 永远不要在 async 函数中使用 `std::sync::Mutex` 跨越 `.await`
- ✅ 使用 `tokio::time::sleep`
- ✅ 使用 `tokio::sync::Mutex`

### 3.3 异步 Trait

**使用 `async-trait` crate**：

```rust
#[async_trait]
pub trait SomeTrait: Send + Sync {
    async fn do_something(&self) -> Result<()>;
}
```

---

## 四、WebSocket 架构原则

### 4.1 请求-响应模式

WebSocket 是全双工协议，没有内置的请求-响应关联。需要自己实现：

```rust
// 使用 correlation ID 关联请求和响应
struct ConfirmationRequest {
    id: String,  // UUID
    // ...
}

struct ConfirmationResponse {
    id: String,  // 对应请求的 ID
    approved: bool,
}

// 使用 HashMap 存储 pending 请求
pending: HashMap<String, oneshot::Sender<bool>>
```

### 4.2 超时处理

**必须实现超时**，否则会永久阻塞：

```rust
match tokio::time::timeout(Duration::from_secs(60), rx).await {
    Ok(result) => { /* 处理结果 */ },
    Err(_) => { /* 超时处理 */ },
}
```

### 4.3 并发处理

使用 `tokio::select!` 同时处理多个事件源：

```rust
loop {
    tokio::select! {
        event = event_rx.recv() => { /* 处理事件 */ },
        msg = websocket.recv() => { /* 处理 WebSocket 消息 */ },
    }
}
```

---

## 五、Claude Code 设计借鉴

### 5.1 三层确认机制

```
Tier 1: 安全工具白名单 → 直接执行
Tier 2: 项目内操作 → 通过版本控制可审查，直接执行
Tier 3: 危险操作 → 需要确认
```

### 5.2 Deny-and-Continue 模式

拒绝后不是中断，而是让 Agent 尝试更安全的方式：

```rust
if !confirmed {
    // 不中断，返回"操作已取消"
    // Agent 可以尝试其他方案
    return Ok("操作已取消".to_string());
}
```

### 5.3 连续拒绝限制

防止 Agent 无限尝试：

```rust
consecutive_denials += 1;
if consecutive_denials >= 3 {
    // 停止执行，要求人工介入
    return Err("连续拒绝次数过多".into());
}
```

---

## 六、代码质量标准

### 6.1 必须通过

- [ ] `cargo build` 无错误
- [ ] `cargo test` 全部通过
- [ ] `cargo clippy` 无警告（或已确认忽略）
- [ ] 所有公开 API 有文档注释

### 6.2 禁止的代码

```rust
// ❌ 禁止：unwrap() 在生产代码中
let value = some_option.unwrap();

// ✅ 正确：显式处理错误
let value = some_option.ok_or_else(|| anyhow!("错误信息"))?;

// ❌ 禁止：硬编码的配置值
const MAX_RETRIES: usize = 3;

// ✅ 正确：从配置读取
config.max_retries

// ❌ 禁止：TODO/FIXME 遗留超过 1 周
// TODO: 以后再实现

// ✅ 正确：创建 Issue 跟踪
// TODO(#123): 实现 XXX 功能
```

### 6.3 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| Trait | 名词，表示能力 | `Interaction`, `ModelProvider` |
| Struct | 名词 | `Session`, `ConfirmationRequest` |
| Function | 动词或动词短语 | `request_confirmation`, `check_interrupt` |
| Async Function | 同上，但返回 `Future` | `async fn process()` |
| Module | 名词 | `interaction`, `session` |

---

## 七、决策记录模板

每次重要决策应记录：

```markdown
## 决策：[标题]

### 背景
[为什么需要做这个决策]

### 选项
1. [选项A]：[描述]
   - 优点：...
   - 缺点：...
2. [选项B]：[描述]
   - 优点：...
   - 缺点：...

### 决策
选择 [选项X]，因为 [原因]

### 参考
- [链接1]
- [链接2]

### 后果
- [需要做的事情]
- [可能的限制]
```

---

## 八、检查清单

每次实现前自问：

1. [ ] 是否搜索了最佳工程实践？
2. [ ] 是否理解了问题的核心矛盾？
3. [ ] 是否选择了最佳方案（不是最简单方案）？
4. [ ] 是否进行了分层抽象？
5. [ ] 是否考虑了扩展性？
6. [ ] 是否考虑了错误处理？
7. [ ] 是否考虑了超时？
8. [ ] 是否考虑了并发安全？
9. [ ] 是否可以增量实现和测试？
10. [ ] 是否达到了生产级质量？

---

**文档版本**: v1.0
**创建日期**: 2026-04-12
**基于**: Sprint 1 Interaction Control Plane 实施经验
