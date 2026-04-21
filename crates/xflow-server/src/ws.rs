//! WebSocket 实时通信 V2
//!
//! 使用新的 XflowEvent 事件系统

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Query, State,
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
use xflow_core::{
    InteractionRequest, OutputEvent, UiAdapter, UserResponse, WebSocketAdapterManager, XflowEvent,
};

/// WebSocket 请求类型
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
    Thinking,
    ThinkingDot,
    ThinkingContent { text: String },
    /// 文本内容（流式）
    Content { text: String },
    /// 工具调用（黄色图标 + 参数）
    ToolCall {
        name: String,
        params_display: String,
        args: serde_json::Value,
    },
    /// 工具结果（内容 + 成功/失败状态）
    ToolResult {
        name: String,
        result: String,
        size: usize,
        success: bool,
    },
    /// 循环进度
    LoopProgress { current: usize, max: usize },
    /// 完成
    Done { tools_called: usize, loops: usize },
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

/// 将 XflowEvent 转换为 WebSocket 响应
fn event_to_response(event: XflowEvent) -> Option<WsResponse> {
    match event {
        XflowEvent::Output(output) => Some(output_to_response(output)),
        XflowEvent::Interaction(InteractionRequest::Confirm(req)) => {
            Some(WsResponse::ConfirmationRequest {
                id: req.id,
                tool: req.tool,
                message: req.message,
                danger_level: req.danger_level,
                danger_reason: req.danger_reason,
            })
        }
        XflowEvent::Interaction(_) => None, // 其他交互类型暂不处理
        XflowEvent::State(_) => None,       // 状态事件暂不发送到前端
    }
}

/// 将 OutputEvent 转换为 WebSocket 响应
fn output_to_response(event: OutputEvent) -> WsResponse {
    match event {
        OutputEvent::ThinkingStart => WsResponse::Thinking,
        OutputEvent::ThinkingDot => WsResponse::ThinkingDot,
        OutputEvent::ThinkingContent { text } => WsResponse::ThinkingContent { text },
        OutputEvent::Content { text } => WsResponse::Content { text },
        OutputEvent::ToolCall {
            name,
            params_display,
            args,
        } => WsResponse::ToolCall {
            name,
            params_display,
            args,
        },
        OutputEvent::ToolResult { name, result } => {
            let result_text = match &result.display {
                xflow_core::ToolResultDisplay::Full { content } => content.clone(),
                xflow_core::ToolResultDisplay::Summary { text } => text.clone(),
                xflow_core::ToolResultDisplay::LineCount { lines, preview } => {
                    format!("{} ({} lines)", preview, lines)
                }
                xflow_core::ToolResultDisplay::ByteSize { size } => size.clone(),
                xflow_core::ToolResultDisplay::StatusOnly => String::new(),
            };
            WsResponse::ToolResult {
                name,
                result: result_text,
                size: result.size,
                success: result.success,
            }
        }
        OutputEvent::Error { message } => WsResponse::Error { message },
        OutputEvent::Done {
            tools_called,
            loops,
        } => WsResponse::Done {
            tools_called,
            loops,
        },
        OutputEvent::LoopProgress { current, max } => WsResponse::LoopProgress { current, max },
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

    // 创建 WebSocket 适配器管理器
    let (ws_manager, ws_adapter) = WebSocketAdapterManager::new();

    // 获取事件接收器
    let mut event_rx = ws_manager.take_event_rx().await;

    // 创建完成信号通道
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<ProcessDone>();

    // 消息处理循环
    loop {
        tokio::select! {
            // 处理事件（流式内容）
            Some(event) = async {
                match event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(response) = event_to_response(event) {
                    if let Ok(json) = serde_json::to_string(&response) {
                        if sender.send(WsMessage::Text(json)).await.is_err() {
                            debug!("WebSocket 发送失败，断开连接");
                            return;
                        }
                    }
                }
            }

            // 处理 process 完成信号
            Some(result) = done_rx.recv() => {
                match result {
                    ProcessDone::Ok => {
                        let done = WsResponse::Done { tools_called: 0, loops: 0 };
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
                                // 获取会话
                                let session_arc = match state.get_session(session_id).await {
                                    Some(s) => s,
                                    None => {
                                        let error = WsResponse::Error {
                                            message: "会话不存在".to_string(),
                                        };
                                        if let Ok(json) = serde_json::to_string(&error) {
                                            let _ = sender.send(WsMessage::Text(json)).await;
                                        }
                                        continue;
                                    }
                                };

                                // 在独立 task 中运行 process
                                let session_clone = session_arc.clone();
                                let message_clone = message.clone();
                                let done_tx_clone = done_tx.clone();
                                let ws_adapter_clone = ws_adapter.clone();

                                tokio::spawn(async move {
                                    let result = {
                                        let mut session = session_clone.lock().await;
                                        // 更新会话的 UI 适配器为 WebSocket 适配器
                                        session.set_ui_adapter(ws_adapter_clone);
                                        session.process(&message_clone).await
                                    };

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

                                let done = WsResponse::Done { tools_called: 0, loops: 0 };
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
                                // 发送确认响应到适配器
                                ws_manager.send_response(UserResponse::Confirm { id, approved }).await;
                            }
                            WsRequest::Interrupt { reason } => {
                                // 发送中断到适配器
                                ws_adapter.interrupt(xflow_core::InterruptInfo::user(reason));
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
