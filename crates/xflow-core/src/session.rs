use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};
use xflow_context::ContextBuilder;
use xflow_model::{Message, ModelProvider};

use crate::agent_executor_impl::SessionAgentExecutor;
use crate::config::XflowConfig;
use crate::tool_loop::{ToolLoop, ToolLoopResult};
use crate::ui_adapter::{AutoConfirmAdapter, UiAdapter};
use xflow_model::get_system_prompt;

/// Token 使用统计
#[derive(Debug, Default)]
pub struct TokenUsage {
    /// 提示词 Token 数
    pub prompt_tokens: u32,
    /// 完成 Token 数
    pub completion_tokens: u32,
    /// 总 Token 数
    pub total_tokens: u32,
    /// 会话 Token 数
    pub session_tokens: u32,
}

impl TokenUsage {
    /// 重置统计
    pub fn reset(&mut self) {
        self.prompt_tokens = 0;
        self.completion_tokens = 0;
        self.total_tokens = 0;
        self.session_tokens = 0;
    }

    /// 更新统计
    pub fn update(&mut self, prompt: u32, completion: u32, total: u32) {
        self.prompt_tokens = prompt;
        self.completion_tokens = completion;
        self.total_tokens = total;
        self.session_tokens += total;
    }

    /// 获取会话总 Token 数
    pub fn session_total(&self) -> u32 {
        self.session_tokens
    }
}

pub struct Session {
    messages: Vec<Message>,
    provider: Arc<dyn ModelProvider>,
    workdir: PathBuf,
    model_name: String,
    /// 工具循环
    tool_loop: ToolLoop,
    ui: Arc<dyn UiAdapter>,
    system_added: bool,
    project_context: Option<String>,
    config: XflowConfig,
    /// Token 使用统计
    token_usage: Arc<Mutex<TokenUsage>>,
}

impl Session {
    pub fn new(provider: Arc<dyn ModelProvider>, workdir: PathBuf, ui: Arc<dyn UiAdapter>) -> Self {
        let model_name = provider.model_info().name;
        let config = XflowConfig::load();

        let executor = Arc::new(SessionAgentExecutor::new(
            provider.clone(),
            ui.clone(),
            xflow_tools::create_default_tools(),
            workdir.clone(),
            config.clone(),
        ));

        let tools = xflow_tools::create_default_tools_with_agent(executor);
        let tool_loop = ToolLoop::new(provider.clone(), tools, ui.clone(), workdir.clone(), config.clone());

        let token_usage = Arc::new(Mutex::new(TokenUsage::default()));
        let token_usage_clone = token_usage.clone();

        let mut session = Self {
            messages: Vec::new(),
            provider,
            workdir,
            model_name,
            tool_loop,
            ui,
            system_added: false,
            project_context: None,
            config,
            token_usage,
        };

        // 设置 token usage 回调
        session.tool_loop = session.tool_loop.with_token_usage_callback(move |prompt, completion, total| {
            if let Ok(mut usage) = token_usage_clone.lock() {
                usage.update(prompt, completion, total);
            }
        });

        session
    }

    pub fn with_auto_confirm(
        provider: Arc<dyn ModelProvider>,
        workdir: PathBuf,
        auto: bool,
    ) -> Self {
        let ui = if auto {
            AutoConfirmAdapter::approving()
        } else {
            AutoConfirmAdapter::rejecting()
        };
        Self::new(provider, workdir, ui)
    }

    pub fn init_project_context(&mut self) -> Result<()> {
        info!("Initializing project context: {:?}", self.workdir);

        let builder = ContextBuilder::new(self.workdir.clone());
        match builder.generate_system_context() {
            Ok(context) => {
                info!("Project context initialized");
                self.project_context = Some(context);
            }
            Err(e) => {
                warn!("Project context initialization failed: {}", e);
            }
        }

        Ok(())
    }

    pub async fn process(&mut self, input: &str) -> Result<ToolLoopResult> {
        if !self.system_added {
            let system_prompt = if let Some(ref context) = self.project_context {
                format!("{}\n{}", get_system_prompt(), context)
            } else {
                get_system_prompt()
            };
            self.messages.push(Message::system(&system_prompt));
            self.system_added = true;
        }

        self.messages.push(Message::user(input));

        debug!("Current message count: {}", self.messages.len());

        self.tool_loop.run(&mut self.messages).await
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.system_added = false;
        if let Ok(mut usage) = self.token_usage.lock() {
            usage.reset();
        }
        info!("Session cleared");
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 获取 Token 使用统计
    pub fn token_usage(&self) -> Arc<Mutex<TokenUsage>> {
        self.token_usage.clone()
    }

    /// 更新 Token 使用统计
    pub fn update_token_usage(&mut self, prompt: u32, completion: u32, total: u32) {
        if let Ok(mut usage) = self.token_usage.lock() {
            usage.update(prompt, completion, total);
        }
    }

    pub fn set_ui_adapter(&mut self, ui: Arc<dyn UiAdapter>) {
        self.tool_loop = ToolLoop::new(
            self.provider.clone(),
            xflow_tools::create_default_tools_with_agent(Arc::new(SessionAgentExecutor::new(
                self.provider.clone(),
                ui.clone(),
                xflow_tools::create_default_tools(),
                self.workdir.clone(),
                self.config.clone(),
            ))),
            ui.clone(),
            self.workdir.clone(),
            self.config.clone(),
        );
        self.ui = ui;
        debug!("UI adapter updated");
    }

    pub fn ui_adapter(&self) -> &Arc<dyn UiAdapter> {
        &self.ui
    }
}
