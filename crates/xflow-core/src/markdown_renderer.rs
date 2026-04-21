//! Markdown 渲染器 V2
//!
//! 支持完整的 Markdown 格式渲染到 ANSI

use once_cell::sync::Lazy;
use regex::Regex;

// 预编译正则表达式（ANSI 渲染）
static CODE_BLOCK_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"```(\w*)\n([\s\S]*?)```").unwrap());
static HEADING6_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^###### (.+)$").unwrap());
static HEADING5_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^##### (.+)$").unwrap());
static HEADING4_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^#### (.+)$").unwrap());
static HEADING3_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^### (.+)$").unwrap());
static HEADING2_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^## (.+)$").unwrap());
static HEADING1_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^# (.+)$").unwrap());
static BOLD_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
static ITALIC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*(.+?)\*").unwrap());
static INLINE_CODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"`(.+?)`").unwrap());
static STRIKETHROUGH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"~~(.+?)~~").unwrap());
static LINK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[(.+?)\]\((.+?)\)").unwrap());
static ULIST_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*[-*] (.+)$").unwrap());
static OLIST_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*(\d+)\. (.+)$").unwrap());
static QUOTE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^> (.+)$").unwrap());
static HR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^---+$").unwrap());

// 预编译正则表达式（HTML 渲染）
static HTML_CODE_BLOCK_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"```(\w*)\n([\s\S]*?)```").unwrap());
static HTML_HEADING6_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^###### (.+)$").unwrap());
static HTML_HEADING5_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^##### (.+)$").unwrap());
static HTML_HEADING4_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^#### (.+)$").unwrap());
static HTML_HEADING3_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^### (.+)$").unwrap());
static HTML_HEADING2_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^## (.+)$").unwrap());
static HTML_HEADING1_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^# (.+)$").unwrap());
static HTML_BOLD_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
static HTML_ITALIC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*(.+?)\*").unwrap());
static HTML_INLINE_CODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"`(.+?)`").unwrap());
static HTML_STRIKETHROUGH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"~~(.+?)~~").unwrap());
static HTML_LINK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[(.+?)\]\((.+?)\)").unwrap());
static HTML_PARA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n\n").unwrap());

/// Markdown 渲染器
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// 创建新渲染器
    pub fn new() -> Self {
        Self
    }

    /// 将 Markdown 渲染为 ANSI 格式（简化版，使用正则）
    pub fn render_to_ansi(&self, markdown: &str) -> String {
        let mut output = markdown.to_string();

        // 代码块
        output = CODE_BLOCK_REGEX
            .replace_all(&output, |caps: &regex::Captures| {
                let _lang = &caps[1];
                let code = &caps[2];
                format!("\n\x1b[48;5;236m{}\x1b[0m\n", code.trim_end())
            })
            .to_string();

        // 标题
        output = HEADING6_REGEX
            .replace_all(&output, "\x1b[1;36m$1\x1b[0m")
            .to_string();
        output = HEADING5_REGEX
            .replace_all(&output, "\x1b[1;36m$1\x1b[0m")
            .to_string();
        output = HEADING4_REGEX
            .replace_all(&output, "\x1b[1;36m$1\x1b[0m")
            .to_string();
        output = HEADING3_REGEX
            .replace_all(&output, "\x1b[1;33m$1\x1b[0m")
            .to_string();
        output = HEADING2_REGEX
            .replace_all(&output, "\x1b[1;33m$1\x1b[0m")
            .to_string();
        output = HEADING1_REGEX
            .replace_all(&output, "\x1b[1;31m$1\x1b[0m")
            .to_string();

        // 粗体
        output = BOLD_REGEX
            .replace_all(&output, "\x1b[1m$1\x1b[0m")
            .to_string();

        // 斜体
        output = ITALIC_REGEX
            .replace_all(&output, "\x1b[3m$1\x1b[0m")
            .to_string();

        // 行内代码（保留反引号作为视觉分隔）
        output = INLINE_CODE_REGEX
            .replace_all(&output, "\x1b[92m`$1`\x1b[0m")
            .to_string();

        // 删除线
        output = STRIKETHROUGH_REGEX
            .replace_all(&output, "\x1b[9m$1\x1b[0m")
            .to_string();

        // 链接
        output = LINK_REGEX
            .replace_all(&output, "\x1b[4;34m$1\x1b[0m \x1b[90m($2)\x1b[0m")
            .to_string();

        // 列表项
        output = ULIST_REGEX
            .replace_all(&output, "  \x1b[90m•\x1b[0m $1")
            .to_string();
        output = OLIST_REGEX
            .replace_all(&output, "  \x1b[90m$1.\x1b[0m $2")
            .to_string();

        // 引用块
        output = QUOTE_REGEX
            .replace_all(&output, "\x1b[90m│\x1b[0m $1")
            .to_string();

        // 水平线
        output = HR_REGEX
            .replace_all(
                &output,
                "\x1b[90m────────────────────────────────────────\x1b[0m",
            )
            .to_string();

        output
    }

    /// 将 Markdown 渲染为 HTML（用于 Web）
    pub fn render_to_html(&self, markdown: &str) -> String {
        // 简单的 HTML 转换
        let mut output = markdown.to_string();

        // 转义 HTML
        output = output.replace('&', "&amp;");
        output = output.replace('<', "&lt;");
        output = output.replace('>', "&gt;");

        // 代码块
        output = HTML_CODE_BLOCK_REGEX
            .replace_all(&output, |caps: &regex::Captures| {
                let lang = &caps[1];
                let code = &caps[2];
                if lang.is_empty() {
                    format!("<pre><code>{}</code></pre>", code.trim_end())
                } else {
                    format!(
                        "<pre><code class=\"language-{}\">{}</code></pre>",
                        lang,
                        code.trim_end()
                    )
                }
            })
            .to_string();

        // 标题
        output = HTML_HEADING6_REGEX
            .replace_all(&output, "<h6>$1</h6>")
            .to_string();
        output = HTML_HEADING5_REGEX
            .replace_all(&output, "<h5>$1</h5>")
            .to_string();
        output = HTML_HEADING4_REGEX
            .replace_all(&output, "<h4>$1</h4>")
            .to_string();
        output = HTML_HEADING3_REGEX
            .replace_all(&output, "<h3>$1</h3>")
            .to_string();
        output = HTML_HEADING2_REGEX
            .replace_all(&output, "<h2>$1</h2>")
            .to_string();
        output = HTML_HEADING1_REGEX
            .replace_all(&output, "<h1>$1</h1>")
            .to_string();

        // 粗体
        output = HTML_BOLD_REGEX
            .replace_all(&output, "<strong>$1</strong>")
            .to_string();

        // 斜体
        output = HTML_ITALIC_REGEX
            .replace_all(&output, "<em>$1</em>")
            .to_string();

        // 行内代码
        output = HTML_INLINE_CODE_REGEX
            .replace_all(&output, "<code>$1</code>")
            .to_string();

        // 删除线
        output = HTML_STRIKETHROUGH_REGEX
            .replace_all(&output, "<del>$1</del>")
            .to_string();

        // 链接
        output = HTML_LINK_REGEX
            .replace_all(&output, r#"<a href="$2">$1</a>"#)
            .to_string();

        // 段落
        output = HTML_PARA_REGEX.replace_all(&output, "</p><p>").to_string();

        format!("<p>{}</p>", output)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// 流式 Markdown 渲染器（兼容旧接口）
pub struct StreamingMarkdownRenderer {
    renderer: MarkdownRenderer,
    buffer: String,
}

impl StreamingMarkdownRenderer {
    pub fn new() -> Self {
        Self {
            renderer: MarkdownRenderer::new(),
            buffer: String::new(),
        }
    }

    pub fn render_chunk(&mut self, text: &str, output_fn: &mut impl FnMut(&str)) {
        self.buffer.push_str(text);

        // 行缓冲策略：遇到换行符就渲染到上一个换行符
        if let Some(last_newline) = self.buffer.rfind('\n') {
            // 提取到最后一个换行符的内容
            let content_to_render = self.buffer[..=last_newline].to_string();
            // 保留剩余内容
            self.buffer = self.buffer[last_newline + 1..].to_string();
            
            let rendered = self.renderer.render_to_ansi(&content_to_render);
            output_fn(&rendered);
        }
    }

    pub fn flush(&mut self, output_fn: &mut impl FnMut(&str)) {
        if !self.buffer.is_empty() {
            let rendered = self.renderer.render_to_ansi(&self.buffer);
            output_fn(&rendered);
            self.buffer.clear();
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for StreamingMarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_heading() {
        let renderer = MarkdownRenderer::new();
        let output = renderer.render_to_ansi("# Hello");
        assert!(output.contains("\x1b[1;31m"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_render_bold() {
        let renderer = MarkdownRenderer::new();
        let output = renderer.render_to_ansi("**bold**");
        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("bold"));
    }

    #[test]
    fn test_render_code() {
        let renderer = MarkdownRenderer::new();
        let output = renderer.render_to_ansi("`code`");
        assert!(output.contains("\x1b[92m"));
        assert!(output.contains("code"));
    }

    #[test]
    fn test_render_to_html() {
        let renderer = MarkdownRenderer::new();
        let output = renderer.render_to_html("# Hello\n\n**bold**");
        assert!(output.contains("<h1>Hello</h1>"), "output was: {}", output);
        assert!(output.contains("<strong>bold</strong>"), "output was: {}", output);
    }
}
