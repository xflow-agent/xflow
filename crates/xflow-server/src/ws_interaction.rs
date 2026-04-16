//! WebSocket 交互实现
//!
//! 通过 WebSocket 与前端交互，支持异步确认

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use xflow_core::{
    Interaction, 
    InteractionContext,
    InteractionEvent,
    InteractionResponse,
    ConfirmationRequest,
    ConfirmationResult,
    InterruptInfo,
};

/// WebSocket 交互实现
///
/// 通过 channel 与 WebSocket handler 通信：
/// - 发送事件到前端 (event_tx)
/// - 接收来自前端的响应 (通过 pending_confirmations)
pub struct WebSocketInteraction {
    /// 交互上下文
    context: InteractionContext,
    /// 事件发送器（发送到 WebSocket）
    event_tx: mpsc::UnboundedSender<InteractionEvent>,
    /// 等待中的确认请求
    pending_confirmations: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
    /// 确认请求超时（秒）
    confirmation_timeout: u64,
}

impl WebSocketInteraction {
    /// 创建新的 WebSocket 交互
    pub fn new(
        event_tx: mpsc::UnboundedSender<InteractionEvent>,
        pending_confirmations: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
    ) -> Self {
        Self {
            context: InteractionContext::new("websocket"),
            event_tx,
            pending_confirmations,
            confirmation_timeout: 60,
        }
    }
    
    /// 设置确认超时
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.confirmation_timeout = timeout_secs;
        self
    }
    
    /// 处理来自前端的响应
    ///
    /// 由 WebSocket handler 调用
    pub async fn handle_response(&self, response: InteractionResponse) {
        match response {
            InteractionResponse::Confirmation { id, approved } => {
                let mut pending = self.pending_confirmations.lock().await;
                if let Some(tx) = pending.remove(&id) {
                    let _ = tx.send(approved);
                }
            }
            InteractionResponse::Interrupt { reason } => {
                self.context.set_interrupt(InterruptInfo {
                    interrupt_type: xflow_core::InterruptType::UserRequested,
                    reason,
                });
            }
        }
    }
    
    /// 获取事件发送器
    pub fn event_tx(&self) -> mpsc::UnboundedSender<InteractionEvent> {
        self.event_tx.clone()
    }
}

#[async_trait::async_trait]
impl Interaction for WebSocketInteraction {
    async fn request_confirmation(&self, req: ConfirmationRequest) -> ConfirmationResult {
        // 1. 创建响应通道
        let (tx, rx) = oneshot::channel();
        
        // 2. 注册等待
        {
            let mut pending = self.pending_confirmations.lock().await;
            pending.insert(req.id.clone(), tx);
        }
        
        // 3. 发送请求到前端
        let _ = self.event_tx.send(InteractionEvent::ConfirmationRequest(req.clone()));
        
        // 4. 等待响应（带超时）
        match tokio::time::timeout(
            Duration::from_secs(self.confirmation_timeout),
            rx
        ).await {
            Ok(Ok(approved)) => {
                if approved {
                    ConfirmationResult::approved()
                } else {
                    ConfirmationResult::cancelled()
                }
            }
            Ok(Err(_)) => {
                // oneshot channel 被关闭
                ConfirmationResult::cancelled()
            }
            Err(_) => {
                // 超时，清理 pending
                let mut pending = self.pending_confirmations.lock().await;
                pending.remove(&req.id);
                ConfirmationResult::timeout()
            }
        }
    }
    
    fn check_interrupt(&self) -> bool {
        self.context.is_interrupted()
    }
    
    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }
    
    fn set_interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }
    
    fn output(&self, content: &str) {
        let _ = self.event_tx.send(InteractionEvent::Output(content.to_string()));
    }
    
    fn output_error(&self, error: &str) {
        let _ = self.event_tx.send(InteractionEvent::Error(error.to_string()));
    }
    
    fn create_child_context(&self, name: &str) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(name),
            event_tx: self.event_tx.clone(),
            pending_confirmations: self.pending_confirmations.clone(),
            confirmation_timeout: self.confirmation_timeout,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(self.context.name()),
            event_tx: self.event_tx.clone(),
            pending_confirmations: self.pending_confirmations.clone(),
            confirmation_timeout: self.confirmation_timeout,
        })
    }
}

/// WebSocket 交互管理器
///
/// 管理 WebSocket 连接的交互状态
pub struct WebSocketInteractionManager {
    /// 事件接收器（给 WebSocket handler 使用）
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<InteractionEvent>>>,
    /// WebSocket 交互实例
    interaction: Arc<WebSocketInteraction>,
}

impl WebSocketInteractionManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let pending_confirmations = Arc::new(Mutex::new(HashMap::new()));
        let interaction = Arc::new(WebSocketInteraction::new(
            event_tx,
            pending_confirmations,
        ));
        
        Self {
            event_rx: Mutex::new(Some(event_rx)),
            interaction,
        }
    }
    
    /// 获取交互实例
    pub fn interaction(&self) -> Arc<WebSocketInteraction> {
        self.interaction.clone()
    }
    
    /// 获取事件接收器
    ///
    /// 只能调用一次
    pub async fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<InteractionEvent>> {
        self.event_rx.lock().await.take()
    }
}

impl Default for WebSocketInteractionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_websocket_interaction_timeout() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let pending_confirmations = Arc::new(Mutex::new(HashMap::new()));
        
        let interaction = WebSocketInteraction::new(event_tx, pending_confirmations)
            .with_timeout(1); // 1 秒超时
        
        let req = ConfirmationRequest::new("test", "test message");
        
        // 不发送响应，应该超时
        let result = interaction.request_confirmation(req).await;
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("超时"));
    }
    
    #[tokio::test]
    async fn test_websocket_interaction_manager() {
        let manager = WebSocketInteractionManager::new();
        let interaction = manager.interaction();
        
        // 可以获取交互实例
        assert!(!interaction.check_interrupt());
        
        // 设置中断
        interaction.set_interrupt(InterruptInfo {
            interrupt_type: xflow_core::InterruptType::UserRequested,
            reason: "test".to_string(),
        });
        
        assert!(interaction.check_interrupt());
    }
}
