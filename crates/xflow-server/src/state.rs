//! 服务器状态管理

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use xflow_core::{AutoConfirmAdapter, Session};
use xflow_model::ModelProvider;

/// 会话 ID
pub type SessionId = Uuid;

/// 服务器状态
pub struct AppState {
    /// 模型提供者
    pub provider: Arc<dyn ModelProvider>,
    /// 工作目录
    pub workdir: PathBuf,
    /// 活跃会话
    pub sessions: RwLock<HashMap<SessionId, Arc<Mutex<Session>>>>,
}

impl AppState {
    /// 创建新的服务器状态
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf) -> Self {
        Self {
            provider,
            workdir,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// 创建新会话
    pub async fn create_session(&self) -> SessionId {
        let id = Uuid::new_v4();
        // 创建默认使用自动确认的适配器（后续会被 WebSocket 适配器替换）
        let ui = AutoConfirmAdapter::approving();
        let session = Session::new(self.provider.clone(), self.workdir.clone(), ui);
        let mut sessions = self.sessions.write().await;
        sessions.insert(id, Arc::new(Mutex::new(session)));
        id
    }

    /// 获取会话
    pub async fn get_session(&self, id: SessionId) -> Option<Arc<Mutex<Session>>> {
        let sessions = self.sessions.read().await;
        sessions.get(&id).cloned()
    }

    /// 删除会话
    pub async fn remove_session(&self, id: SessionId) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&id);
    }

    /// 获取会话数量
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}
