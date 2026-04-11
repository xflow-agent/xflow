//! REPL 交互模块

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{DefaultEditor, Editor};
use std::path::Path;
use std::sync::Arc;
use xflow_core::Session;
use xflow_model::OllamaProvider;

/// REPL 交互界面
pub struct Repl {
    editor: Editor<(), DefaultHistory>,
    session: Session,
}

impl Repl {
    /// 创建新的 REPL 实例
    pub fn new(model: &str, host: &str, workdir: &Path) -> Result<Self> {
        // 初始化编辑器
        let mut editor = DefaultEditor::new()
            .context("无法初始化 rustyline 编辑器")?;

        // 加载历史记录
        let history_path = dirs::data_dir()
            .map(|p| p.join("xflow/history.txt"))
            .unwrap_or_else(|| Path::new(".xflow_history").to_path_buf());
        
        if history_path.exists() {
            let _ = editor.load_history(&history_path);
        }

        // 初始化模型提供者
        let provider = Arc::new(OllamaProvider::new(host.to_string(), model.to_string()));

        // 初始化会话
        let session = Session::new(provider, workdir.to_path_buf());

        // 打印欢迎信息
        print_welcome();

        Ok(Self { editor, session })
    }

    /// 运行 REPL 主循环
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // 读取用户输入
            let readline = self.editor.readline("xflow> ");

            match readline {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // 添加到历史
                    let _ = self.editor.add_history_entry(line);

                    // 处理特殊命令
                    if self.handle_command(line).await? {
                        continue;
                    }

                    // 处理用户输入
                    self.session.process(line).await?;
                }
                Err(ReadlineError::Interrupted) => {
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
fn print_welcome() {
    println!(
        r#"
╔═════════════════════════════════════════╗
║           xflow - 心流编程助手           ║
║                                         ║
║  输入 /help 查看帮助                    ║
║  输入 /exit 退出                        ║
╚═════════════════════════════════════════╝
"#
    );
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

使用方法:
  直接输入问题或指令，AI 会帮助您完成编程任务。
"#
    );
}
