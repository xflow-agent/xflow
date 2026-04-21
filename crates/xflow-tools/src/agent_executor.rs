use async_trait::async_trait;

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute_agent(&self, tool_name: &str, args: serde_json::Value) -> String;
}
