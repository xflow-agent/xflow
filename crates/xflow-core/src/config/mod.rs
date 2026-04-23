use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XflowConfig {
    pub model: ModelConfig,
    pub tools: ToolsConfig,
    pub session: SessionConfig,
    pub agent: AgentConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model_name: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: env_or("XFLOW_BASE_URL", "http://localhost:11434/v1"),
            api_key: std::env::var("XFLOW_API_KEY").ok(),
            model_name: env_or("XFLOW_MODEL", "gemma4:e4b"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub max_tool_result_size: usize,
    pub max_tool_loops: usize,
    pub confirm_dangerous: bool,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_tool_result_size: env_or_parse("XFLOW_MAX_TOOL_RESULT_SIZE", 10000),
            max_tool_loops: env_or_parse("XFLOW_MAX_TOOL_LOOPS", 20),
            confirm_dangerous: env_or_parse("XFLOW_CONFIRM_DANGEROUS", true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub max_message_history: usize,
    pub dot_animation_max: usize,
    pub max_context_tokens: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_message_history: env_or_parse("XFLOW_MAX_MESSAGE_HISTORY", 100),
            dot_animation_max: env_or_parse("XFLOW_DOT_ANIMATION_MAX", 60),
            max_context_tokens: env_or_parse("XFLOW_MAX_CONTEXT_TOKENS", 128000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub execution_timeout: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            execution_timeout: env_or_parse("XFLOW_EXECUTION_TIMEOUT", 300u64),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub show_thinking: bool,
    pub verbose_errors: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_thinking: env_or_parse("XFLOW_SHOW_THINKING", true),
            verbose_errors: env_or_parse("XFLOW_VERBOSE_ERRORS", false),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_or_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl XflowConfig {
    pub fn load() -> Self {
        Self::default()
    }

    pub fn model(&self) -> &ModelConfig {
        &self.model
    }

    pub fn tools(&self) -> &ToolsConfig {
        &self.tools
    }

    pub fn session(&self) -> &SessionConfig {
        &self.session
    }

    pub fn agent(&self) -> &AgentConfig {
        &self.agent
    }

    pub fn ui(&self) -> &UiConfig {
        &self.ui
    }

    pub fn workdir(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = XflowConfig::default();
        assert_eq!(config.tools.max_tool_loops, 20);
        assert_eq!(config.agent.execution_timeout, 300);
        assert!(config.ui.show_thinking);
        assert!(!config.ui.verbose_errors);
    }
}
