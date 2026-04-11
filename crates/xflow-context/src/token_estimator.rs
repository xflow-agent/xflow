//! Token 估算器
//!
//! 提供简单的 Token 数量估算功能

/// Token 估算器
pub struct TokenEstimator {
    /// 每个字符平均 Token 数（经验值）
    /// 英文约 0.25 tokens/char，中文约 0.5 tokens/char
    chars_per_token: f64,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenEstimator {
    /// 创建新的估算器
    pub fn new() -> Self {
        // 使用保守估计：约 4 字符 = 1 token
        Self {
            chars_per_token: 4.0,
        }
    }
    
    /// 估算文本的 token 数量
    pub fn estimate(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        
        // 计算中文字符数
        let chinese_chars = text.chars().filter(|c| {
            (*c >= '\u{4E00}' && *c <= '\u{9FFF}') || // CJK Unified Ideographs
            (*c >= '\u{3400}' && *c <= '\u{4DBF}') || // CJK Extension A
            (*c >= '\u{20000}' && *c <= '\u{2A6DF}') // CJK Extension B
        }).count();
        
        // 计算非中文字符数
        let other_chars = text.chars().count() - chinese_chars;
        
        // 中文约 1.5 字符/token，英文约 4 字符/token
        let chinese_tokens = (chinese_chars as f64 / 1.5).ceil() as usize;
        let other_tokens = (other_chars as f64 / self.chars_per_token).ceil() as usize;
        
        chinese_tokens + other_tokens
    }
    
    /// 估算多条消息的总 token 数
    pub fn estimate_messages(&self, messages: &[&str]) -> usize {
        messages.iter().map(|m| self.estimate(m)).sum()
    }
    
    /// 检查是否超出预算
    pub fn is_over_budget(&self, text: &str, budget: usize) -> bool {
        self.estimate(text) > budget
    }
    
    /// 截断文本到指定 token 数
    pub fn truncate(&self, text: &str, max_tokens: usize) -> String {
        if text.is_empty() {
            return String::new();
        }
        
        let estimated = self.estimate(text);
        if estimated <= max_tokens {
            return text.to_string();
        }
        
        // 按比例估算需要保留的字符数
        let ratio = max_tokens as f64 / estimated as f64;
        let target_chars = (text.chars().count() as f64 * ratio) as usize;
        
        // 截断并添加省略号
        let truncated: String = text.chars().take(target_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// 便捷函数：估算文本的 token 数量
pub fn estimate_tokens(text: &str) -> usize {
    TokenEstimator::new().estimate(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_estimate_empty() {
        let estimator = TokenEstimator::new();
        assert_eq!(estimator.estimate(""), 0);
    }
    
    #[test]
    fn test_estimate_english() {
        let estimator = TokenEstimator::new();
        // "Hello, world!" = 13 chars ≈ 3-4 tokens
        let tokens = estimator.estimate("Hello, world!");
        assert!(tokens >= 3 && tokens <= 4);
    }
    
    #[test]
    fn test_estimate_chinese() {
        let estimator = TokenEstimator::new();
        // "你好世界" = 4 Chinese chars ≈ 3 tokens
        let tokens = estimator.estimate("你好世界");
        assert!(tokens >= 2 && tokens <= 4);
    }
    
    #[test]
    fn test_estimate_mixed() {
        let estimator = TokenEstimator::new();
        // "Hello 你好" = 6 English + 2 Chinese + 1 space
        let tokens = estimator.estimate("Hello 你好");
        assert!(tokens > 0);
    }
    
    #[test]
    fn test_truncate() {
        let estimator = TokenEstimator::new();
        let text = "This is a long text that should be truncated to fit within the token budget.";
        let truncated = estimator.truncate(text, 10);
        assert!(estimator.estimate(&truncated) <= 15); // 允许一些误差
    }
}
