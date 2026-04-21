//! REPL 交互模块

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Cmd, Completer, Editor, Helper, Highlighter, Hinter, KeyCode, KeyEvent, Modifiers};
use std::path::Path;
use std::sync::Arc;
use xflow_core::{CliAdapter, InterruptInfo, ModelProvider, Session};

// ── rustyline Helper：支持多行编辑 ──────────────────────────────

/// xflow 的 rustyline Helper，启用多行编辑支持
///
/// 多行输入方式：
/// - Alt+Enter / Ctrl+J：插入换行（不提交）
/// - Enter：提交输入
/// - 反斜杠 \ 结尾：自动续行
#[derive(Helper, Completer, Hinter, Highlighter)]
struct XflowHelper;

impl Validator for XflowHelper {
    /// 验证输入是否完整：反斜杠结尾时视为续行，Enter 换行继续输入
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();
        // 去掉末尾空白后，以反斜杠结尾则视为续行
        if input.trim_end().ends_with('\\') {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

/// REPL 交互界面
pub struct Repl {
    editor: Editor<XflowHelper, DefaultHistory>,
    session: Session,
}

impl Repl {
    /// 创建新的 REPL 实例
    pub fn new(provider: Arc<dyn ModelProvider>, model: &str, workdir: &Path) -> Result<Self> {
        // 初始化编辑器（使用 XflowHelper 支持多行）
        let mut editor = Editor::new().context("无法初始化 rustyline 编辑器")?;
        editor.set_helper(Some(XflowHelper));

        // Alt+Enter：插入换行（不提交）
        editor.bind_sequence(
            KeyEvent(KeyCode::Enter, Modifiers::ALT),
            Cmd::Newline,
        );
        // Ctrl+J：插入换行（不提交），作为 Alt+Enter 的 fallback
        editor.bind_sequence(KeyEvent::ctrl('j'), Cmd::Newline);

        // 加载历史记录
        let history_path = dirs::data_dir()
            .map(|p| p.join("xflow/history.txt"))
            .unwrap_or_else(|| Path::new(".xflow_history").to_path_buf());

        if history_path.exists() {
            let _ = editor.load_history(&history_path);
        }

        // 创建 CLI 适配器
        let ui_adapter = CliAdapter::new();

        // 初始化会话
        let mut session = Session::new(provider, workdir.to_path_buf(), ui_adapter);

        // 打印欢迎信息（显示完整工作目录和模型）
        print_welcome(workdir, model);

        // 初始化项目上下文（静默初始化）
        if let Err(e) = session.init_project_context() {
            tracing::warn!("项目上下文初始化失败: {}", e);
        }

        Ok(Self { editor, session })
    }

    /// 运行 REPL 主循环
    pub async fn run(&mut self) -> Result<()> {
        // 注册 Ctrl+C handler：设置中断标志而非终止进程
        let ui = self.session.ui_adapter().clone();
        ctrlc::set_handler(move || {
            ui.interrupt(InterruptInfo::user("Ctrl+C"));
        })
        .context("无法注册 Ctrl+C 处理器")?;

        loop {
            // 每次循环开始前清除之前的中断标志
            self.session.ui_adapter().clear_interrupt();

            // 读取用户输入
            let readline = self.editor.readline("\x1b[1;36mxflow>\x1b[0m ");

            match readline {
                Ok(line) => {
                    // 多行输入只去除首尾空白行，保留内部换行和缩进
                    let line = line.trim_start_matches('\n').trim_end();
                    if line.is_empty() {
                        continue;
                    }

                    // 添加到历史
                    let _ = self.editor.add_history_entry(line);

                    // 处理特殊命令
                    if self.handle_command(line).await? {
                        continue;
                    }

                    // 处理输入（Agent 工具已集成到工具系统中）
                    self.session.process(line).await?;
                }
                Err(ReadlineError::Interrupted) => {
                    // 在输入等待时按 Ctrl+C，rustyline 会捕获
                    // 这里不需要额外处理，因为 session.process 期间的中断
                    // 由 ctrlc handler 处理
                    println!("\n使用 /exit 或 Ctrl-D 退出");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("再见!");
                    break;
                }
                Err(err) => {
                    anyhow::bail!("读取错误: {}", err);
                }
            }
        }

        // 保存历史
        let history_path = dirs::data_dir()
            .map(|p| p.join("xflow/history.txt"))
            .unwrap_or_else(|| Path::new(".xflow_history").to_path_buf());

        if let Some(parent) = history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = self.editor.save_history(&history_path);

        Ok(())
    }

    /// 处理特殊命令
    async fn handle_command(&mut self, line: &str) -> Result<bool> {
        match line {
            "/exit" | "/quit" | "/q" => {
                println!("再见!");
                std::process::exit(0);
            }
            "/help" | "/h" | "/?" => {
                print_help();
                return Ok(true);
            }
            "/clear" => {
                self.session.clear();
                println!("会话已清空");
                return Ok(true);
            }
            "/model" => {
                println!("当前模型: {}", self.session.model_name());
                return Ok(true);
            }
            _ if line.starts_with('/') => {
                println!("未知命令: {}，使用 /help 查看帮助", line);
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}

/// 打印欢迎信息
fn print_welcome(workdir: &Path, model: &str) {
    // ASCII art logo - XFLOW (更清晰的字体)
    let logo: [&str; 6] = [
        "██╗  ██╗███████╗██╗      ██████╗ ██╗    ██╗",
        "╚██╗██╔╝██╔════╝██║     ██╔═══██╗██║    ██║",
        " ╚███╔╝ █████╗  ██║     ██║   ██║██║ █╗ ██║",
        " ██╔██╗ ██╔══╝  ██║     ██║   ██║██║███╗██║",
        "██╔╝ ██╗██║     ███████╗╚██████╔╝╚███╔███╔╝",
        "╚═╝  ╚═╝╚═╝     ╚══════╝ ╚═════╝  ╚══╝╚══╝ ",
    ];

    // 右侧信息 (与logo顶部对齐)
    let version = env!("CARGO_PKG_VERSION");
    let info: [String; 6] = [
        "\x1b[1;36mXFlow\x1b[0m 心流 AI 编程助手".to_string(),
        format!("\x1b[90m版本号: {}\x1b[0m", version),
        String::new(),
        format!("\x1b[90m当前模型:\x1b[0m {}", model),
        format!("\x1b[90m工作目录:\x1b[0m {}", workdir.display()),
        String::new(),
    ];

    println!();

    // 逐行输出 logo + info
    for i in 0..logo.len() {
        let left = logo[i];
        let right = &info[i];
        println!("{}  {}", left, right);
    }

    println!();
    println!("  Hi～今天想做点什么？");
    println!();
}

/// 打印帮助信息
fn print_help() {
    println!(
        r#"
命令:
  /help, /h, /?    显示帮助
  /exit, /quit, /q 退出
  /clear           清空会话
  /model           显示当前模型

多行输入:
  Alt+Enter / Ctrl+J  插入换行（不提交）
  Enter               提交输入
  行尾 \              续行（下次 Enter 继续输入）
"#
    );
}
