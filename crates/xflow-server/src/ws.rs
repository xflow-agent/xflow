//! WebSocket 实时通信

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
use xflow_core::OutputMessage;

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
}

/// WebSocket 响应类型
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsResponse {
    /// 文本内容（流式）
    Content { text: String },
    /// 工具调用
    ToolCall { name: String },
    /// 工具结果
    ToolResult { name: String, size: usize },
    /// 循环进度
    LoopProgress { current: usize, max: usize },
    /// 完成
    Done,
    /// 错误
    Error { message: String },
    /// 心跳响应
    Pong,
    /// 会话信息
    SessionInfo { session_id: SessionId },
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
fn to_response(msg: OutputMessage) -> WsResponse {
    match msg {
        OutputMessage::Content(text) => WsResponse::Content { text },
        OutputMessage::ToolCall { name, .. } => WsResponse::ToolCall { name },
        OutputMessage::ToolResult { name, result_size } => WsResponse::ToolResult { 
            name, 
            size: result_size 
        },
        OutputMessage::LoopProgress { current, max } => WsResponse::LoopProgress { current, max },
        OutputMessage::Done { .. } => WsResponse::Done,
        OutputMessage::Error(text) => WsResponse::Error { message: text },
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
    
    // 消息处理循环
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(WsMessage::Text(text)) => {
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
                        // 创建输出通道
                        let (output_tx, mut output_rx) = 
                            tokio::sync::mpsc::unbounded_channel::<OutputMessage>();
                        
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
                        
                        // 设置回调（需要 lock）
                        {
                            let mut session = session_arc.lock().await;
                            session.set_output(xflow_core::realtime_callback(output_tx.clone()));
                            session.set_auto_confirm(true);
                        }
                        
                        // 在独立任务中运行 process
                        let mut process_handle = tokio::spawn(async move {
                            let mut session = session_arc.lock().await;
                            session.process(&message).await
                        });
                        
                        // 使用 select! 同时处理输出和等待 process 完成
                        loop {
                            tokio::select! {
                                // 接收输出并发送
                                output_msg = output_rx.recv() => {
                                    match output_msg {
                                        Some(msg) => {
                                            let response = to_response(msg);
                                            if let Ok(json) = serde_json::to_string(&response) {
                                                if sender.send(WsMessage::Text(json)).await.is_err() {
                                                    debug!("WebSocket 发送失败，断开连接");
                                                    return;
                                                }
                                            }
                                        }
                                        None => {
                                            // 通道关闭，所有消息已发送
                                            break;
                                        }
                                    }
                                }
                                
                                // 等待 process 完成
                                result = &mut process_handle => {
                                    // 发送所有剩余消息
                                    while let Ok(msg) = output_rx.try_recv() {
                                        let response = to_response(msg);
                                        if let Ok(json) = serde_json::to_string(&response) {
                                            if sender.send(WsMessage::Text(json)).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    
                                    // 发送完成/错误信号
                                    match result {
                                        Ok(Ok(_)) => {
                                            let done = WsResponse::Done;
                                            if let Ok(json) = serde_json::to_string(&done) {
                                                let _ = sender.send(WsMessage::Text(json)).await;
                                            }
                                        }
                                        Ok(Err(e)) => {
                                            let error = WsResponse::Error { 
                                                message: e.to_string() 
                                            };
                                            if let Ok(json) = serde_json::to_string(&error) {
                                                let _ = sender.send(WsMessage::Text(json)).await;
                                            }
                                        }
                                        Err(e) => {
                                            let error = WsResponse::Error { 
                                                message: format!("任务错误: {}", e) 
                                            };
                                            if let Ok(json) = serde_json::to_string(&error) {
                                                let _ = sender.send(WsMessage::Text(json)).await;
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                        }
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
                }
            }
            Ok(WsMessage::Close(_)) => {
                info!("WebSocket 连接关闭");
                break;
            }
            Ok(WsMessage::Ping(data)) => {
                let _ = sender.send(WsMessage::Pong(data)).await;
            }
            Err(e) => {
                warn!("WebSocket 错误: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    debug!("WebSocket 连接结束, session_id: {}", session_id);
}