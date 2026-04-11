//! xflow CLI - AI 编程助手
//!
//! 一个类似 Claude Code 的智能编程工具

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;

mod repl;

use repl::Repl;

/// xflow - 心流 AI 编程助手
#[derive(Parser, Debug)]
#[command(name = "xflow", author, version, about, long_about = None)]
struct Args {
    /// 模型名称 (Ollama 格式)
    #[arg(short, long, default_value = "gemma4:e4b")]
    model: String,

    /// Ollama 服务地址
    #[arg(long, default_value = "http://localhost:11434")]
    host: String,

    /// 工作目录
    #[arg(short, long, default_value = ".")]
    workdir: PathBuf,

    /// 调试模式
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 初始化日志
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    info!("启动 xflow...");
    info!("模型: {}", args.model);
    info!("服务: {}", args.host);
    info!("目录: {:?}", args.workdir);

    // 启动 REPL
    let mut repl = Repl::new(&args.model, &args.host, &args.workdir)?;
    repl.run().await?;

    Ok(())
}
