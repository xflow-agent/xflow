//! REST API 路由

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::{AppState, SessionId};

/// API 错误响应
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

/// 创建会话响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: SessionId,
}

/// 发送消息请求
#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub message: String,
}

/// 发送消息响应
#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub response: String,
}

/// 会话信息
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub message_count: usize,
    pub model: String,
}

/// 会话列表响应
#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

/// 创建 API 路由
pub fn create_api_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/:id", get(get_session_info))
        .route("/sessions/:id/chat", post(send_message))
        .route("/sessions/:id/clear", post(clear_session))
        .route("/sessions/:id", axum::routing::delete(delete_session))
        .with_state(state)
}

/// POST /sessions - 创建新会话
async fn create_session(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    let session_id = state.create_session().await;
    Ok(Json(CreateSessionResponse { session_id }))
}

/// GET /sessions - 列出所有会话
async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<ListSessionsResponse> {
    let sessions = state.sessions.read().await;
    let session_list: Vec<SessionInfo> = sessions
        .iter()
        .map(|(id, session)| {
            // 尝试获取会话信息（非阻塞）
            let session = session.try_lock();
            match session {
                Ok(s) => SessionInfo {
                    id: *id,
                    message_count: s.message_count(),
                    model: s.model_name().to_string(),
                },
                Err(_) => SessionInfo {
                    id: *id,
                    message_count: 0,
                    model: "unknown".to_string(),
                },
            }
        })
        .collect();

    Json(ListSessionsResponse {
        sessions: session_list,
    })
}

/// GET /sessions/:id - 获取会话信息
async fn get_session_info(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SessionId>,
) -> Result<Json<SessionInfo>, ApiError> {
    let session = state.get_session(id).await.ok_or_else(|| ApiError {
        error: "会话不存在".to_string(),
    })?;

    let session = session.lock().await;
    Ok(Json(SessionInfo {
        id,
        message_count: session.message_count(),
        model: session.model_name().to_string(),
    }))
}

/// POST /sessions/:id/chat - 发送消息
async fn send_message(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SessionId>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ApiError> {
    let session = state.get_session(id).await.ok_or_else(|| ApiError {
        error: "会话不存在".to_string(),
    })?;

    let mut session = session.lock().await;

    match session.process(&req.message).await {
        Ok(_) => {
            Ok(Json(SendMessageResponse {
                response: "消息已处理".to_string(),
            }))
        }
        Err(e) => Err(ApiError {
            error: format!("处理消息失败: {}", e),
        }),
    }
}

/// POST /sessions/:id/clear - 清空会话
async fn clear_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state.get_session(id).await.ok_or_else(|| ApiError {
        error: "会话不存在".to_string(),
    })?;

    let mut session = session.lock().await;
    session.clear();

    Ok(Json(serde_json::json!({ "success": true })))
}

/// DELETE /sessions/:id - 删除会话
async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SessionId>,
) -> Json<serde_json::Value> {
    state.remove_session(id).await;
    Json(serde_json::json!({ "success": true }))
}
