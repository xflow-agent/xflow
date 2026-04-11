//! REPL 交互模块

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{DefaultEditor, Editor};
use std::path::Path;
use std::sync::Arc;
use xflow_core::Session;
use xflow_model::OllamaProvider;
use xflow_agent::{AgentCoordinator, AgentType};

/// REPL 交互界面
pub struct Repl {
    editor: Editor<(), DefaultHistory>,
    session: Session,
    /// Agent 模式开关
    agent_mode: bool,
    /// Agent 协调器
    coordinator: AgentCoordinator,
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
        let mut session = Session::new(provider.clone(), workdir.to_path_buf());
        
        // 初始化项目上下文
        if let Err(e) = session.init_project_context() {
            tracing::warn!("项目上下文初始化失败: {}", e);
        }

        // 初始化 Agent 协调器
        let coordinator = AgentCoordinator::new(provider, workdir.to_path_buf());

        // 打印欢迎信息
        print_welcome();

        Ok(Self { 
            editor, 
            session,
            agent_mode: false,
            coordinator,
        })
    }

    /// 运行 REPL 主循环
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // 读取用户输入
            let prompt = if self.agent_mode { "xflow [agent]> " } else { "xflow> " };
            let readline = self.editor.readline(prompt);

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

                    // 根据模式处理输入
                    if self.agent_mode {
                        self.process_with_agent(line).await?;
                    } else {
                        self.session.process(line).await?;
                    }
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

    /// 使用 Agent 系统处理请求
    async fn process_with_agent(&mut self, input: &str) -> Result<()> {
        println!("\n{}", "─".repeat(50));
        println!("🤖 Agent 模式 - 多 Agent 协作");
        println!("{}", "─".repeat(50));
        
        match self.coordinator.process(input).await {
            Ok(result) => {
                result.print_summary();
            }
            Err(e) => {
                println!("\n❌ Agent 执行失败: {}", e);
            }
        }
        
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
            "/agent" | "/agents" => {
                self.agent_mode = !self.agent_mode;
                if self.agent_mode {
                    println!("🤖 已切换到 Agent 模式");
                    println!("   可用 Agent: {:?}", self.coordinator.available_agents());
                    println!("   输入任务，系统会自动分配合适的 Agent 处理");
                } else {
                    println!("已切换回普通模式");
                }
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
║  输入 /agent 切换 Agent 模式            ║
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
  /agent, /agents  切换 Agent 模式

Agent 模式:
  在 Agent 模式下，系统会根据任务类型自动选择合适的 Agent:
  - PlannerAgent: 任务分解
  - CoderAgent: 代码编写
  - ReviewerAgent: 代码审查

使用方法:
  直接输入问题或指令，AI 会帮助您完成编程任务。
"#
    );
}
