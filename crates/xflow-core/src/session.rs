use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};
use xflow_context::ContextBuilder;
use xflow_model::{Message, ModelProvider};

use crate::agent_executor_impl::SessionAgentExecutor;
use crate::config::XflowConfig;
use crate::tool_loop::{ToolLoop, ToolLoopResult};
use crate::ui_adapter::{AutoConfirmAdapter, UiAdapter};
use xflow_model::get_system_prompt;

pub struct Session {
    messages: Vec<Message>,
    provider: Arc<dyn ModelProvider>,
    workdir: PathBuf,
    model_name: String,
    tool_loop: ToolLoop,
    ui: Arc<dyn UiAdapter>,
    system_added: bool,
    project_context: Option<String>,
    config: XflowConfig,
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

        Self {
            messages: Vec::new(),
            provider,
            workdir,
            model_name,
            tool_loop,
            ui,
            system_added: false,
            project_context: None,
            config,
        }
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
        info!("Session cleared");
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
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
