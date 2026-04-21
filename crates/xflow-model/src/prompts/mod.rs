mod agent;
mod system;

pub use agent::*;
pub use system::*;

pub fn get_system_prompt() -> String {
    system::SYSTEM_PROMPT.to_string()
}

pub fn get_reviewer_prompt() -> String {
    agent::REVIEWER_SYSTEM_PROMPT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_prompt() {
        let prompt = get_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("xflow"));
    }

    #[test]
    fn test_get_reviewer_prompt() {
        let prompt = get_reviewer_prompt();
        assert!(!prompt.is_empty());
    }
}
