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
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, info, warn};

use crate::state::{AppState, SessionId};
use xflow_core::{OutputCallback, OutputMessage};

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

/// WebSocket 升级处理
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, query))
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
    let session_info = WsResponse::SessionInfo { session_id };
    if let Ok(json) = serde_json::to_string(&session_info) {
        let _ = sender.send(WsMessage::Text(json)).await;
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
                        // 创建消息队列（用于收集 Session 输出）
                        let output_queue: Arc<StdMutex<Vec<OutputMessage>>> = Arc::new(StdMutex::new(Vec::new()));
                        let queue_clone = output_queue.clone();
                        
                        // 创建输出回调
                        let output_callback: OutputCallback = Box::new(move |msg| {
                            if let Ok(mut queue) = queue_clone.lock() {
                                queue.push(msg);
                            }
                        });
                        
                        // 获取会话并处理
                        if let Some(session) = state.get_session(session_id).await {
                            let mut session = session.lock().await;
                            session.set_output(output_callback);
                            session.set_auto_confirm(true); // Web 端自动确认
                            
                            // 处理消息
                            if let Err(e) = session.process(&message).await {
                                let error = WsResponse::Error { message: e.to_string() };
                                if let Ok(json) = serde_json::to_string(&error) {
                                    let _ = sender.send(WsMessage::Text(json)).await;
                                }
                            }
                        }
                        
                        // 发送收集的消息
                        let messages: Vec<OutputMessage> = {
                            if let Ok(mut queue) = output_queue.lock() {
                                std::mem::take(&mut *queue)
                            } else {
                                Vec::new()
                            }
                        };
                        
                        for msg in messages {
                            let response = to_response(msg);
                            if let Ok(json) = serde_json::to_string(&response) {
                                let _ = sender.send(WsMessage::Text(json)).await;
                            }
                        }
                        
                        // 发送完成信号
                        let done = WsResponse::Done;
                        if let Ok(json) = serde_json::to_string(&done) {
                            let _ = sender.send(WsMessage::Text(json)).await;
                        }
                    }
                    WsRequest::Clear => {
                        if let Some(session) = state.get_session(session_id).await {
                            let mut session = session.lock().await;
                            session.clear();
                        }
                        
                        // 发送确认
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