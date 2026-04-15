//! REPL 交互模块

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{DefaultEditor, Editor};
use std::path::Path;
use std::sync::Arc;
use xflow_core::Session;
use xflow_model::ModelProvider;

/// REPL 交互界面
pub struct Repl {
    editor: Editor<(), DefaultHistory>,
    session: Session,
}

impl Repl {
    /// 创建新的 REPL 实例
    pub fn new(provider: Arc<dyn ModelProvider>, model: &str, workdir: &Path) -> Result<Self> {
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

        // 初始化会话
        let mut session = Session::new(provider, workdir.to_path_buf());

        // 获取完整的工作目录路径
        let full_workdir = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
        
        // 打印欢迎信息（显示完整工作目录和模型）
        print_welcome(&full_workdir, model);
        
        // 初始化项目上下文（静默初始化）
        if let Err(e) = session.init_project_context() {
            tracing::warn!("项目上下文初始化失败: {}", e);
        }

        Ok(Self { 
            editor, 
            session,
        })
    }

    /// 运行 REPL 主循环
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // 读取用户输入
            let readline = self.editor.readline("\x1b[1;36mxflow>\x1b[0m ");

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

                    // 处理输入（Agent 工具已集成到工具系统中）
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

高级工具:
  AI 会自动判断并调用以下高级工具：
  - analyze_project: 项目分析（"分析项目"、"分析功能"）
  - implement_feature: 功能实现（"实现xxx功能"）

使用方法:
  直接输入问题或指令，AI 会自动选择合适的工具完成你的任务。
  
示例:
  "分析一下这个项目的所有功能"
  "实现一个用户登录功能"
  "修复 src/main.rs 中的编译错误"
"#
    );
}
