//! UI 适配器 trait
//!
//! 定义 Session 与 UI 层交互的统一接口

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::events::*;

/// 交互请求发送器类型
pub type InteractionSender =
    mpsc::UnboundedSender<(InteractionRequest, oneshot::Sender<UserResponse>)>;

/// 交互请求接收器类型
pub type InteractionReceiver =
    mpsc::UnboundedReceiver<(InteractionRequest, oneshot::Sender<UserResponse>)>;

/// UI 适配器 trait
///
/// 这是 Session 与 UI 层交互的核心抽象。
/// 不同 UI 实现（CLI、Web、Headless）只需实现此 trait。
#[async_trait]
pub trait UiAdapter: Send + Sync {
    /// 发送事件到 UI
    async fn emit(&self, event: XflowEvent);

    /// 发送输出事件（便捷方法）
    async fn output(&self, event: OutputEvent) {
        self.emit(XflowEvent::Output(event)).await;
    }

    /// 发送交互请求并等待响应
    async fn request(&self, request: InteractionRequest) -> Option<UserResponse>;

    /// 请求确认（便捷方法）
    async fn confirm(&self, request: ConfirmationRequest) -> bool {
        match self.request(InteractionRequest::Confirm(request)).await {
            Some(UserResponse::Confirm { approved, .. }) => approved,
            _ => false,
        }
    }

    /// 检查是否被中断
    fn is_interrupted(&self) -> bool;

    /// 获取中断信息
    fn get_interrupt_info(&self) -> Option<InterruptInfo>;

    /// 设置中断
    fn interrupt(&self, info: InterruptInfo);

    /// 清除中断
    fn clear_interrupt(&self);

    /// 创建子上下文（用于 SubAgent）
    fn create_child(&self, name: &str) -> Arc<dyn UiAdapter>;
}

/// 适配器上下文 - 共享状态
#[derive(Debug)]
pub struct AdapterContext {
    /// 中断标志
    interrupt: std::sync::atomic::AtomicBool,
    /// 中断信息
    interrupt_info: std::sync::Mutex<Option<InterruptInfo>>,
    /// 上下文名称
    name: String,
}

impl AdapterContext {
    /// 创建新的上下文
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            interrupt: std::sync::atomic::AtomicBool::new(false),
            interrupt_info: std::sync::Mutex::new(None),
            name: name.into(),
        }
    }

    /// 从父上下文创建子上下文
    pub fn child(&self, name: impl Into<String>) -> Self {
        Self {
            interrupt: std::sync::atomic::AtomicBool::new(false),
            interrupt_info: std::sync::Mutex::new(None),
            name: name.into(),
        }
    }

    /// 获取名称
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 检查是否中断
    pub fn is_interrupted(&self) -> bool {
        self.interrupt.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取中断信息
    pub fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.interrupt_info.lock().unwrap().clone()
    }

    /// 设置中断
    pub fn set_interrupt(&self, info: InterruptInfo) {
        self.interrupt
            .store(true, std::sync::atomic::Ordering::Relaxed);
        *self.interrupt_info.lock().unwrap() = Some(info);
    }

    /// 清除中断
    pub fn clear_interrupt(&self) {
        self.interrupt
            .store(false, std::sync::atomic::Ordering::Relaxed);
        *self.interrupt_info.lock().unwrap() = None;
    }
}

/// 自动确认适配器（用于测试和 headless 模式）
pub struct AutoConfirmAdapter {
    context: AdapterContext,
    auto_approve: bool,
}

impl AutoConfirmAdapter {
    /// 创建自动批准的适配器
    pub fn approving() -> Arc<Self> {
        Arc::new(Self {
            context: AdapterContext::new("auto-approve"),
            auto_approve: true,
        })
    }

    /// 创建自动拒绝的适配器
    pub fn rejecting() -> Arc<Self> {
        Arc::new(Self {
            context: AdapterContext::new("auto-reject"),
            auto_approve: false,
        })
    }
}

#[async_trait]
impl UiAdapter for AutoConfirmAdapter {
    async fn emit(&self, _event: XflowEvent) {
        // 忽略所有输出事件
    }

    async fn request(&self, request: InteractionRequest) -> Option<UserResponse> {
        match request {
            InteractionRequest::Confirm(req) => Some(UserResponse::Confirm {
                id: req.id,
                approved: self.auto_approve,
            }),
            _ => None,
        }
    }

    fn is_interrupted(&self) -> bool {
        self.context.is_interrupted()
    }

    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }

    fn interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }

    fn clear_interrupt(&self) {
        self.context.clear_interrupt();
    }

    fn create_child(&self, name: &str) -> Arc<dyn UiAdapter> {
        Arc::new(Self {
            context: self.context.child(name),
            auto_approve: self.auto_approve,
        })
    }
}

/// 通道适配器 - 基于 mpsc 通道的通用实现
pub struct ChannelAdapter {
    context: AdapterContext,
    event_tx: mpsc::UnboundedSender<XflowEvent>,
    response_tx: tokio::sync::Mutex<Option<InteractionSender>>,
}

impl ChannelAdapter {
    /// 创建新的通道适配器
    pub fn new(
        name: impl Into<String>,
        event_tx: mpsc::UnboundedSender<XflowEvent>,
    ) -> (Arc<Self>, InteractionReceiver) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        let adapter = Arc::new(Self {
            context: AdapterContext::new(name),
            event_tx,
            response_tx: tokio::sync::Mutex::new(Some(response_tx)),
        });

        (adapter, response_rx)
    }

    /// 获取事件发送器
    pub fn event_tx(&self) -> mpsc::UnboundedSender<XflowEvent> {
        self.event_tx.clone()
    }
}

#[async_trait]
impl UiAdapter for ChannelAdapter {
    async fn emit(&self, event: XflowEvent) {
        let _ = self.event_tx.send(event);
    }

    async fn request(&self, request: InteractionRequest) -> Option<UserResponse> {
        let (tx, rx) = oneshot::channel();

        let response_tx = self.response_tx.lock().await;
        if let Some(ref sender) = *response_tx {
            if sender.send((request, tx)).is_ok() {
                return rx.await.ok();
            }
        }

        None
    }

    fn is_interrupted(&self) -> bool {
        self.context.is_interrupted()
    }

    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }

    fn interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }

    fn clear_interrupt(&self) {
        self.context.clear_interrupt();
    }

    fn create_child(&self, name: &str) -> Arc<dyn UiAdapter> {
        Arc::new(Self {
            context: self.context.child(name),
            event_tx: self.event_tx.clone(),
            response_tx: tokio::sync::Mutex::new(None), // 子上下文不处理请求
        })
    }
}
