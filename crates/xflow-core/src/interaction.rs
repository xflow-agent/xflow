//! 交互控制平面
//!
//! 提供抽象的交互接口，支持多种 UI 适配器（CLI、Web 等）
//! 解决确认、中断、SubAgent 交互等核心问题

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 确认请求
#[derive(Debug, Clone)]
pub struct ConfirmationRequest {
    /// 唯一标识符
    pub id: String,
    /// 工具名称
    pub tool: String,
    /// 操作描述
    pub message: String,
    /// 危险等级 (0-3)
    /// - 0: 需要确认但非危险
    /// - 1: 中度危险
    /// - 2: 高度危险
    /// - 3: 极度危险
    pub danger_level: u8,
    /// 危险原因
    pub danger_reason: Option<String>,
}

impl ConfirmationRequest {
    /// 创建新的确认请求
    pub fn new(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tool: tool.into(),
            message: message.into(),
            danger_level: 0,
            danger_reason: None,
        }
    }
    
    /// 设置危险等级
    pub fn with_danger(mut self, level: u8, reason: impl Into<String>) -> Self {
        self.danger_level = level;
        self.danger_reason = Some(reason.into());
        self
    }
}

/// 确认结果
#[derive(Debug, Clone)]
pub struct ConfirmationResult {
    /// 是否批准
    pub approved: bool,
    /// 拒绝原因（可选）
    pub reason: Option<String>,
}

impl ConfirmationResult {
    /// 批准
    pub fn approved() -> Self {
        Self { approved: true, reason: None }
    }
    
    /// 拒绝
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self { approved: false, reason: Some(reason.into()) }
    }
    
    /// 超时
    pub fn timeout() -> Self {
        Self { 
            approved: false, 
            reason: Some("确认超时".to_string()) 
        }
    }
    
    /// 取消
    pub fn cancelled() -> Self {
        Self { 
            approved: false, 
            reason: Some("操作已取消".to_string()) 
        }
    }
}

/// 中断类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptType {
    /// 用户请求中断
    UserRequested,
    /// 超时中断
    Timeout,
    /// 错误中断
    Error,
    /// 系统中断
    System,
}

/// 中断信息
#[derive(Debug, Clone)]
pub struct InterruptInfo {
    /// 中断类型
    pub interrupt_type: InterruptType,
    /// 中断原因
    pub reason: String,
}

/// 交互事件（发送到 UI）
#[derive(Debug, Clone)]
pub enum InteractionEvent {
    /// 确认请求
    ConfirmationRequest(ConfirmationRequest),
    /// 输出内容
    Output(String),
    /// 错误
    Error(String),
}

/// 交互响应（从 UI 返回）
#[derive(Debug, Clone)]
pub enum InteractionResponse {
    /// 确认响应
    Confirmation { id: String, approved: bool },
    /// 中断请求
    Interrupt { reason: String },
}

/// 交互接口 (Port)
///
/// 这是 Session 与 UI 层交互的核心抽象。
/// 不同 UI 实现（CLI、Web、Headless）只需实现此 trait。
#[async_trait::async_trait]
pub trait Interaction: Send + Sync {
    /// 请求确认
    ///
    /// 阻塞直到收到响应或超时
    async fn request_confirmation(&self, req: ConfirmationRequest) -> ConfirmationResult;
    
    /// 请求重试
    ///
    /// 当操作失败时询问用户是否重试
    async fn request_retry(&self, error_msg: &str) -> bool {
        // 默认实现：显示错误并询问是否重试
        println!("\n\x1b[31m✗ {}\x1b[0m", error_msg);
        
        match inquire::Confirm::new("是否重试?")
            .with_default(false)
            .prompt()
        {
            Ok(true) => true,
            Ok(false) => false,
            Err(e) => {
                tracing::warn!("重试确认错误：{}", e);
                false
            }
        }
    }
    
    /// 检查是否被中断
    fn check_interrupt(&self) -> bool;
    
    /// 获取中断信息
    fn get_interrupt_info(&self) -> Option<InterruptInfo>;
    
    /// 设置中断标志
    fn set_interrupt(&self, info: InterruptInfo);
    
    /// 输出内容
    fn output(&self, content: &str);
    
    /// 输出错误
    fn output_error(&self, error: &str);
    
    /// 创建子上下文
    ///
    /// SubAgent 使用此方法获取受限的交互上下文
    fn create_child_context(&self, name: &str) -> Box<dyn Interaction>;
    
    /// 克隆为 trait object
    fn clone_box(&self) -> Box<dyn Interaction>;
}

/// Clone 实现 for Box<dyn Interaction>
impl Clone for Box<dyn Interaction> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// 基础交互上下文
///
/// 提供共享的中断标志等基础功能
pub struct InteractionContext {
    /// 中断标志
    interrupt_flag: Arc<AtomicBool>,
    /// 中断信息
    interrupt_info: Arc<std::sync::Mutex<Option<InterruptInfo>>>,
    /// 上下文名称（用于调试）
    name: String,
}

impl InteractionContext {
    /// 创建新的交互上下文
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            interrupt_flag: Arc::new(AtomicBool::new(false)),
            interrupt_info: Arc::new(std::sync::Mutex::new(None)),
            name: name.into(),
        }
    }
    
    /// 从父上下文创建子上下文
    pub fn child(&self, name: impl Into<String>) -> Self {
        Self {
            interrupt_flag: self.interrupt_flag.clone(),
            interrupt_info: self.interrupt_info.clone(),
            name: name.into(),
        }
    }
    
    /// 获取上下文名称
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// 检查中断
    pub fn is_interrupted(&self) -> bool {
        self.interrupt_flag.load(Ordering::Relaxed)
    }
    
    /// 获取中断信息
    pub fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.interrupt_info.lock().unwrap().clone()
    }
    
    /// 设置中断
    pub fn set_interrupt(&self, info: InterruptInfo) {
        self.interrupt_flag.store(true, Ordering::Relaxed);
        *self.interrupt_info.lock().unwrap() = Some(info);
    }
    
    /// 清除中断
    pub fn clear_interrupt(&self) {
        self.interrupt_flag.store(false, Ordering::Relaxed);
        *self.interrupt_info.lock().unwrap() = None;
    }
}

/// 自动确认交互 (用于测试和 headless 模式)
pub struct AutoConfirmInteraction {
    context: InteractionContext,
    auto_approve: bool,
}

impl AutoConfirmInteraction {
    /// 创建自动批准的交互
    pub fn approving() -> Self {
        Self {
            context: InteractionContext::new("auto-approve"),
            auto_approve: true,
        }
    }
    
    /// 创建自动拒绝的交互
    pub fn rejecting() -> Self {
        Self {
            context: InteractionContext::new("auto-reject"),
            auto_approve: false,
        }
    }
}

#[async_trait::async_trait]
impl Interaction for AutoConfirmInteraction {
    async fn request_confirmation(&self, _req: ConfirmationRequest) -> ConfirmationResult {
        if self.auto_approve {
            ConfirmationResult::approved()
        } else {
            ConfirmationResult::rejected("自动拒绝")
        }
    }
    
    async fn request_retry(&self, _error_msg: &str) -> bool {
        // 自动确认模式下不重试
        false
    }
    
    fn check_interrupt(&self) -> bool {
        self.context.is_interrupted()
    }
    
    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }
    
    fn set_interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }
    
    fn output(&self, _content: &str) {
        // 忽略输出
    }
    
    fn output_error(&self, _error: &str) {
        // 忽略错误
    }
    
    fn create_child_context(&self, name: &str) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(name),
            auto_approve: self.auto_approve,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(&self.context.name),
            auto_approve: self.auto_approve,
        })
    }
}

// ==================== CLI 交互实现 ====================

/// CLI 交互实现
///
/// 使用 inquire 在终端显示对话框
pub struct CliInteraction {
    context: InteractionContext,
    /// 输出回调
    output_fn: Box<dyn Fn(&str) + Send + Sync>,
}

impl CliInteraction {
    /// 创建新的 CLI 交互
    pub fn new() -> Self {
        Self {
            context: InteractionContext::new("cli"),
            output_fn: Box::new(|s| print!("{}", s)),
        }
    }
    
    /// 创建带自定义输出的 CLI 交互
    pub fn with_output(output_fn: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            context: InteractionContext::new("cli"),
            output_fn: Box::new(output_fn),
        }
    }
}

impl Default for CliInteraction {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Interaction for CliInteraction {
    async fn request_confirmation(&self, req: ConfirmationRequest) -> ConfirmationResult {
        // 显示工具名（带图标和缩进）
        println!();
        
        // 显示危险等级（如果有）
        if req.danger_level > 0 {
            let level_display = match req.danger_level {
                3 => "🔴 极度危险",
                2 => "🟠 高度危险",
                1 => "🟡 中度危险",
                _ => "⚠️ 需要注意",
            };
            if let Some(ref reason) = req.danger_reason {
                println!("  \x1b[33m⚠️  {} - {}\x1b[0m", level_display, reason);
            } else {
                println!("  \x1b[33m⚠️  {}\x1b[0m", level_display);
            }
        }
        
        // 显示详情信息（灰色，带缩进）
        println!("  \x1b[90m工具:\x1b[0m {}", req.tool);
        if !req.message.is_empty() {
            for line in req.message.lines() {
                println!("  \x1b[90m{}\x1b[0m", line);
            }
        }

        // 使用 inquire 进行确认，默认 Yes
        let confirm_msg = if req.danger_level > 0 {
            "⚠️  确认执行此危险操作?"
        } else {
            "是否执行此操作?"
        };
        
        match inquire::Confirm::new(confirm_msg)
            .with_default(true)  // 默认 Yes
            .prompt()
        {
            Ok(true) => {
                println!("  \x1b[32m✓ 执行操作...\x1b[0m");
                ConfirmationResult::approved()
            }
            Ok(false) => {
                println!("  \x1b[33m✗ 已取消\x1b[0m");
                ConfirmationResult::cancelled()
            }
            Err(e) => {
                tracing::warn!("确认对话框错误: {}", e);
                println!("  \x1b[31m✗ 确认失败，已取消\x1b[0m");
                ConfirmationResult::rejected(e.to_string())
            }
        }
    }
    
    fn check_interrupt(&self) -> bool {
        self.context.is_interrupted()
    }
    
    fn get_interrupt_info(&self) -> Option<InterruptInfo> {
        self.context.get_interrupt_info()
    }
    
    fn set_interrupt(&self, info: InterruptInfo) {
        self.context.set_interrupt(info);
    }
    
    fn output(&self, content: &str) {
        (self.output_fn)(content);
    }
    
    async fn request_retry(&self, error_msg: &str) -> bool {
        // 显示错误信息（带颜色）
        println!("\n\x1b[31m✗ 操作失败\x1b[0m");
        for line in error_msg.lines() {
            println!("  \x1b[90m{}\x1b[0m", line);
        }
        
        // 询问是否重试
        match inquire::Confirm::new("\n是否重试？")
            .with_default(false)
            .prompt()
        {
            Ok(true) => {
                println!("  \x1b[32m✓ 重新尝试...\x1b[0m");
                true
            }
            Ok(false) => {
                println!("  \x1b[33m✗ 已取消\x1b[0m");
                false
            }
            Err(e) => {
                tracing::warn!("重试确认错误：{}", e);
                false
            }
        }
    }
    
    fn output_error(&self, error: &str) {
        println!("\n[错误: {}]", error);
    }
    
    fn create_child_context(&self, name: &str) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(name),
            output_fn: Box::new(|s| print!("{}", s)),
        })
    }
    
    fn clone_box(&self) -> Box<dyn Interaction> {
        Box::new(Self {
            context: self.context.child(&self.context.name),
            output_fn: Box::new(|s| print!("{}", s)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_confirmation_request() {
        let req = ConfirmationRequest::new("run_shell", "rm -rf /")
            .with_danger(3, "删除系统文件");
        
        assert_eq!(req.tool, "run_shell");
        assert_eq!(req.danger_level, 3);
        assert!(req.danger_reason.is_some());
    }
    
    #[test]
    fn test_auto_confirm_interaction() {
        let interaction = AutoConfirmInteraction::approving();
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            interaction.request_confirmation(
                ConfirmationRequest::new("test", "test message")
            ).await
        });
        
        assert!(result.approved);
    }
    
    #[test]
    fn test_interrupt_context() {
        let ctx = InteractionContext::new("test");
        assert!(!ctx.is_interrupted());
        
        ctx.set_interrupt(InterruptInfo {
            interrupt_type: InterruptType::UserRequested,
            reason: "用户取消".to_string(),
        });
        
        assert!(ctx.is_interrupted());
        let info = ctx.get_interrupt_info().unwrap();
        assert_eq!(info.interrupt_type, InterruptType::UserRequested);
    }
}
