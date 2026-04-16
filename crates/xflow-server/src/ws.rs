//! WebSocket 实时通信
//!
//! 实现 Interaction Control Plane 的 WebSocket 适配器

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State, Query,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::state::{AppState, SessionId};
use crate::ws_interaction::WebSocketInteractionManager;
use xflow_core::{OutputMessage, InteractionEvent, InteractionResponse, Interaction};

/// WebSocket 消息类型
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsRequest {
    /// 发送聊天消息
    Chat { message: String },
    /// 清空会话
    Clear,
    /// 心跳
    Ping,
    /// 确认响应
    ConfirmationResponse { id: String, approved: bool },
    /// 中断请求
    Interrupt { reason: String },
}

/// WebSocket 响应类型
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsResponse {
    /// 思考中状态
    Thinking,
    /// 思考内容（流式，灰色斜体）
    ThinkingContent { text: String },
    /// 文本内容（流式）
    Content { text: String },
    /// 工具调用（黄色图标 + 参数）
    ToolCall { name: String, params_display: String },
    /// 工具结果（内容 + 成功/失败状态）
    ToolResult { name: String, result: String, size: usize, success: bool },
    /// 循环进度（不显示，仅用于内部状态）
    LoopProgress { current: usize, max: usize },
    /// 完成
    Done,
    /// 错误
    Error { message: String },
    /// 心跳响应
    Pong,
    /// 会话信息
    SessionInfo { session_id: SessionId },
    /// 确认请求
    ConfirmationRequest {
        id: String,
        tool: String,
        message: String,
        danger_level: u8,
        danger_reason: Option<String>,
    },
    /// 进度更新
    Progress { phase: String, current: usize, total: usize, message: String },
}

/// WebSocket 连接参数
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub session_id: Option<SessionId>,
}

/// 创建 WebSocket 路由
pub fn create_ws_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// 将输出消息转换为 WebSocket 响应
fn output_to_response(msg: OutputMessage) -> WsResponse {
    match msg {
        OutputMessage::Thinking => WsResponse::Thinking,
        OutputMessage::ThinkingContent(text) => WsResponse::ThinkingContent { text },
        OutputMessage::Content(text) => WsResponse::Content { text },
        OutputMessage::ToolCall { name, args: _, params_display } => WsResponse::ToolCall { 
            name, 
            params_display 
        },
        OutputMessage::ToolResult { name, result, result_size, success } => WsResponse::ToolResult { 
            name, 
            result,
            size: result_size,
            success,
        },
        OutputMessage::LoopProgress { current, max } => WsResponse::LoopProgress { current, max },
        OutputMessage::Done { .. } => WsResponse::Done,
        OutputMessage::Error(text) => WsResponse::Error { message: text },
    }
}

/// 将交互事件转换为 WebSocket 响应
fn event_to_response(event: InteractionEvent) -> WsResponse {
    match event {
        InteractionEvent::ConfirmationRequest(req) => WsResponse::ConfirmationRequest {
            id: req.id,
            tool: req.tool,
            message: req.message,
            danger_level: req.danger_level,
            danger_reason: req.danger_reason,
        },
        InteractionEvent::Progress(progress) => WsResponse::Progress {
            phase: format!("{:?}", progress.phase),
            current: progress.current,
            total: progress.total,
            message: progress.message,
        },
        InteractionEvent::Output(text) => WsResponse::Content { text },
        InteractionEvent::Error(text) => WsResponse::Error { message: text },
    }
}

/// WebSocket 升级处理
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, query))
}

/// 处理完成结果
enum ProcessDone {
    Ok,
    Error(String),
}

/// 处理 WebSocket 连接
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, query: WsQuery) {
    let (mut sender, mut receiver) = socket.split();
    
    // 获取或创建会话
    let session_id = if let Some(id) = query.session_id {
        if state.get_session(id).await.is_some() {
            id
        } else {
            state.create_session().await
        }
    } else {
        state.create_session().await
    };
    
    info!("WebSocket 连接建立, session_id: {}", session_id);
    
    // 发送会话信息
    {
        let session_info = WsResponse::SessionInfo { session_id };
        if let Ok(json) = serde_json::to_string(&session_info) {
            let _ = sender.send(WsMessage::Text(json)).await;
        }
    }
    
    // 创建输出通道（用于流式内容）
    let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel::<OutputMessage>();
    
    // 创建完成信号通道（用于通知 process 完成）
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<ProcessDone>();
    
    // 创建 WebSocket 交互管理器
    let interaction_manager = Arc::new(WebSocketInteractionManager::new());
    let interaction = interaction_manager.interaction();
    
    // 获取交互事件接收器
    let mut event_rx = interaction_manager.take_event_rx().await;
    
    // 消息处理循环
    loop {
        tokio::select! {
            // 处理输出消息（流式内容）
            Some(msg) = output_rx.recv() => {
                let response = output_to_response(msg);
                if let Ok(json) = serde_json::to_string(&response) {
                    if sender.send(WsMessage::Text(json)).await.is_err() {
                        debug!("WebSocket 发送失败，断开连接");
                        return;
                    }
                }
            }
            
            // 处理交互事件（确认请求、进度等）
            Some(event) = async {
                match event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let response = event_to_response(event);
                if let Ok(json) = serde_json::to_string(&response) {
                    if sender.send(WsMessage::Text(json)).await.is_err() {
                        debug!("WebSocket 发送失败，断开连接");
                        return;
                    }
                }
            }
            
            // 处理 process 完成信号
            Some(result) = done_rx.recv() => {
                match result {
                    ProcessDone::Ok => {
                        let done = WsResponse::Done;
                        if let Ok(json) = serde_json::to_string(&done) {
                            let _ = sender.send(WsMessage::Text(json)).await;
                        }
                    }
                    ProcessDone::Error(msg) => {
                        let error = WsResponse::Error { message: msg };
                        if let Ok(json) = serde_json::to_string(&error) {
                            let _ = sender.send(WsMessage::Text(json)).await;
                        }
                    }
                }
            }
            
            // 处理 WebSocket 消息
            msg = receiver.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        debug!("收到 WebSocket 消息: {}", text);
                        
                        // 解析消息
                        let request: WsRequest = match serde_json::from_str(&text) {
                            Ok(r) => r,
                            Err(e) => {
                                warn!("解析消息失败: {}", e);
                                let error = WsResponse::Error {
                                    message: format!("无效的消息格式: {}", e),
                                };
                                if let Ok(json) = serde_json::to_string(&error) {
                                    let _ = sender.send(WsMessage::Text(json)).await;
                                }
                                continue;
                            }
                        };
                        
                        // 处理请求
                        match request {
                            WsRequest::Chat { message } => {
                                // 获取会话 Arc
                                let session_arc = match state.get_session(session_id).await {
                                    Some(s) => s,
                                    None => {
                                        let error = WsResponse::Error { 
                                            message: "会话不存在".to_string() 
                                        };
                                        if let Ok(json) = serde_json::to_string(&error) {
                                            let _ = sender.send(WsMessage::Text(json)).await;
                                        }
                                        continue;
                                    }
                                };
                                
                                // 设置输出回调和交互接口
                                {
                                    let mut session = session_arc.lock().await;
                                    session.set_output(xflow_core::realtime_callback(output_tx.clone()));
                                    // 使用 WebSocket 交互的 clone_box 方法
                                    session.set_interaction((*interaction).clone_box());
                                }
                                
                                // 在独立 task 中运行 process，避免阻塞 tokio::select!
                                // 这样可以让 select! 循环继续处理确认响应等事件
                                let session_clone = session_arc.clone();
                                let message_clone = message.clone();
                                let done_tx_clone = done_tx.clone();
                                tokio::spawn(async move {
                                    let result = {
                                        let mut session = session_clone.lock().await;
                                        session.process(&message_clone).await
                                    };
                                    // 发送完成信号
                                    match result {
                                        Ok(()) => {
                                            let _ = done_tx_clone.send(ProcessDone::Ok);
                                        }
                                        Err(e) => {
                                            let _ = done_tx_clone.send(ProcessDone::Error(e.to_string()));
                                        }
                                    }
                                });
                            }
                            WsRequest::Clear => {
                                if let Some(session) = state.get_session(session_id).await {
                                    let mut session = session.lock().await;
                                    session.clear();
                                }
                                
                                let done = WsResponse::Done;
                                if let Ok(json) = serde_json::to_string(&done) {
                                    let _ = sender.send(WsMessage::Text(json)).await;
                                }
                            }
                            WsRequest::Ping => {
                                let pong = WsResponse::Pong;
                                if let Ok(json) = serde_json::to_string(&pong) {
                                    let _ = sender.send(WsMessage::Text(json)).await;
                                }
                            }
                            WsRequest::ConfirmationResponse { id, approved } => {
                                // 处理确认响应
                                interaction.handle_response(InteractionResponse::Confirmation { 
                                    id, 
                                    approved 
                                }).await;
                            }
                            WsRequest::Interrupt { reason } => {
                                // 处理中断请求
                                interaction.handle_response(InteractionResponse::Interrupt { 
                                    reason 
                                }).await;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        info!("WebSocket 连接关闭");
                        break;
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        let _ = sender.send(WsMessage::Pong(data)).await;
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket 错误: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }
    
    debug!("WebSocket 连接结束, session_id: {}", session_id);
}
