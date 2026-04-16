//! xflow CLI - AI 编程助手
//!
//! 一个类似 Claude Code 的智能编程工具

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

mod repl;

use repl::Repl;
use xflow_model::{ModelProvider, OpenAIProvider};

/// xflow - 心流 AI 编程助手
#[derive(Parser, Debug)]
#[command(name = "xflow", author, version, about, long_about = None)]
struct Args {
    /// API 基础 URL
    /// - vLLM: http://localhost:8000/v1
    /// - OpenAI: https://api.openai.com/v1
    /// - Ollama: http://localhost:11434/v1
    #[arg(short, long, default_value = "http://172.21.246.100:8000/v1")]
    base_url: String,

    /// API Key (OpenAI/vLLM 可选)
    #[arg(short = 'k', long, default_value = "sk-kmgyfhiuf2imftm9l2w0kdw3sgquqbab")]
    api_key: Option<String>,

    /// 模型名称
    #[arg(short, long, default_value = "qwen3.5-27b")]
    model: String,

    /// 工作目录
    #[arg(short, long, default_value = ".")]
    workdir: PathBuf,

    /// 调试模式（输出日志到控制台）
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 初始化日志
    init_logging(args.debug);

    info!("启动 xflow...");
    info!("Base URL: {}", args.base_url);
    info!("模型：{}", args.model);
    info!("目录：{:?}", args.workdir);

    // 创建模型提供者 (OpenAI 兼容模式)
    let provider: Arc<dyn ModelProvider> = Arc::new(OpenAIProvider::new(
        args.base_url.clone(),
        args.api_key.clone(),
        args.model.clone(),
        "openai-compatible".to_string(),
    ));

    // 启动 REPL
    let mut repl = Repl::new(provider, &args.model, &args.workdir)?;
    repl.run().await?;

    Ok(())
}

/// 初始化日志系统
fn init_logging(debug: bool) {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    use chrono::Local;
    
    // 日志文件路径 - 使用日期命名：xflow-20260416.log
    let log_dir = dirs::data_dir()
        .map(|p| p.join("xflow/logs"))
        .unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&log_dir);
    
    // 获取当前日期，格式：20260416
    let date_str = Local::now().format("%Y%m%d").to_string();
    let log_file = log_dir.join(format!("xflow-{}.log", date_str));
    
    // 自定义时间格式：2026-04-14 15:11:45.823
    let time_format = tracing_subscriber::fmt::time::ChronoLocal::new(
        "%Y-%m-%d %H:%M:%S%.3f".parse().unwrap()
    );
    
    // 创建文件日志层
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap()))
        .with_ansi(false)
        .with_timer(time_format.clone());
    
    if debug {
        // 调试模式：同时输出到文件和控制台
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_timer(time_format);
        
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")))
            .with(file_layer)
            .with(console_layer)
            .init();
    } else {
        // 正常模式：只输出到文件
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
            .with(file_layer)
            .init();
    }
}
