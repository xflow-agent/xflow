//! WebSocket 适配器实现
//!
//! 将事件通过 WebSocket 发送到前端，支持异步确认

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::events::*;
use crate::ui_adapter::{AdapterContext, UiAdapter};

/// WebSocket 适配器
///
/// 通过 mpsc 通道与 WebSocket handler 通信
pub struct WebSocketAdapter {
    context: AdapterContext,
    /// 事件发送器（发送到 WebSocket）
    event_tx: mpsc::UnboundedSender<XflowEvent>,
    /// 等待中的确认请求
    pending_confirmations: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
    /// 确认请求超时（秒）
    confirmation_timeout: u64,
}

impl WebSocketAdapter {
    /// 创建新的 WebSocket 适配器
    pub fn new(
        event_tx: mpsc::UnboundedSender<XflowEvent>,
    ) -> (
        Arc<Self>,
        mpsc::UnboundedSender<(InteractionRequest, oneshot::Sender<UserResponse>)>,
    ) {
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();
        let pending_confirmations = Arc::new(Mutex::new(HashMap::new()));

        let adapter = Arc::new(Self {
            context: AdapterContext::new("websocket"),
            event_tx,
            pending_confirmations: pending_confirmations.clone(),
            confirmation_timeout: 60,
        });

        // 启动响应处理任务
        let adapter_clone = adapter.clone();
        tokio::spawn(async move {
            while let Some((request, tx)) = response_rx.recv().await {
                adapter_clone.handle_request(request, tx).await;
            }
        });

        (adapter, response_tx)
    }

    /// 设置确认超时
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.confirmation_timeout = timeout_secs;
        self
    }

    /// 处理交互请求
    async fn handle_request(&self, request: InteractionRequest, tx: oneshot::Sender<UserResponse>) {
        match request {
            InteractionRequest::Confirm(req) => {
                // 发送确认请求到前端
                let _ = self
                    .event_tx
                    .send(XflowEvent::Interaction(InteractionRequest::Confirm(
                        req.clone(),
                    )));

                // 注册等待
                let (confirm_tx, confirm_rx) = oneshot::channel();
                {
                    let mut pending = self.pending_confirmations.lock().await;
                    pending.insert(req.id.clone(), confirm_tx);
                }

                // 等待响应或超时
                let result = match tokio::time::timeout(
                    Duration::from_secs(self.confirmation_timeout),
                    confirm_rx,
                )
                .await
                {
                    Ok(Ok(approved)) => UserResponse::Confirm {
                        id: req.id,
                        approved,
                    },
                    Ok(Err(_)) => UserResponse::Confirm {
                        id: req.id,
                        approved: false,
                    },
                    Err(_) => {
                        // 超时，清理 pending
                        let mut pending = self.pending_confirmations.lock().await;
                        pending.remove(&req.id);
                        UserResponse::Confirm {
                            id: req.id,
                            approved: false,
                        }
                    }
                };

                let _ = tx.send(result);
            }
            InteractionRequest::Input { prompt } => {
                let _ = self
                    .event_tx
                    .send(XflowEvent::Interaction(InteractionRequest::Input {
                        prompt,
                    }));
                // Web 端暂不支持输入，直接返回空
                let _ = tx.send(UserResponse::Input {
                    text: String::new(),
                });
            }
            InteractionRequest::Select { options, prompt } => {
                let _ = self
                    .event_tx
                    .send(XflowEvent::Interaction(InteractionRequest::Select {
                        options,
                        prompt,
                    }));
                // Web 端暂不支持选择，直接返回第一个
                let _ = tx.send(UserResponse::Select { index: 0 });
            }
        }
    }

    /// 处理来自前端的响应
    ///
    /// 由 WebSocket handler 调用
    pub async fn handle_response(&self, response: UserResponse) {
        match response {
            UserResponse::Confirm { id, approved } => {
                let mut pending = self.pending_confirmations.lock().await;
                if let Some(tx) = pending.remove(&id) {
                    let _ = tx.send(approved);
                }
            }
            _ => {
                // 其他响应类型暂不处理
            }
        }
    }

    /// 获取事件发送器
    pub fn event_tx(&self) -> mpsc::UnboundedSender<XflowEvent> {
        self.event_tx.clone()
    }
}

#[async_trait]
impl UiAdapter for WebSocketAdapter {
    async fn emit(&self, event: XflowEvent) {
        let _ = self.event_tx.send(event);
    }

    async fn request(&self, request: InteractionRequest) -> Option<UserResponse> {
        let (tx, rx) = oneshot::channel();
        self.handle_request(request, tx).await;
        rx.await.ok()
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
        Arc::new(WebSocketAdapter {
            context: self.context.child(name),
            event_tx: self.event_tx.clone(),
            pending_confirmations: self.pending_confirmations.clone(),
            confirmation_timeout: self.confirmation_timeout,
        })
    }
}

/// WebSocket 适配器管理器
///
/// 管理 WebSocket 连接的适配器状态
pub struct WebSocketAdapterManager {
    /// 事件接收器（给 WebSocket handler 使用）
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<XflowEvent>>>,
    /// 适配器实例
    adapter: Arc<WebSocketAdapter>,
}

impl WebSocketAdapterManager {
    /// 创建新的管理器
    pub fn new() -> (Self, Arc<WebSocketAdapter>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (adapter, _response_tx) = WebSocketAdapter::new(event_tx);

        let manager = Self {
            event_rx: Mutex::new(Some(event_rx)),
            adapter: adapter.clone(),
        };

        (manager, adapter)
    }

    /// 获取适配器实例
    pub fn adapter(&self) -> Arc<WebSocketAdapter> {
        self.adapter.clone()
    }

    /// 获取事件接收器
    ///
    /// 只能调用一次
    pub async fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<XflowEvent>> {
        self.event_rx.lock().await.take()
    }

    /// 发送响应到适配器
    pub async fn send_response(&self, response: UserResponse) {
        self.adapter.handle_response(response).await;
    }
}

impl Default for WebSocketAdapterManager {
    fn default() -> Self {
        let (manager, _) = Self::new();
        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_adapter_event() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (adapter, _) = WebSocketAdapter::new(tx);

        // 发送事件
        adapter
            .emit(XflowEvent::Output(OutputEvent::ThinkingStart))
            .await;

        // 验证收到事件
        let event = rx.recv().await;
        assert!(matches!(
            event,
            Some(XflowEvent::Output(OutputEvent::ThinkingStart))
        ));
    }

    #[tokio::test]
    async fn test_websocket_adapter_response() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (adapter, _) = WebSocketAdapter::new(tx);

        // 模拟发送确认响应
        adapter
            .handle_response(UserResponse::Confirm {
                id: "test-id".to_string(),
                approved: true,
            })
            .await;

        // 由于没有 pending 的请求，这里不会 panic
    }
}
