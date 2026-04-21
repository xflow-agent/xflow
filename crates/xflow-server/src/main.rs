//! xflow Web API 服务器入口

use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::{routing::get, Router};
use clap::Parser;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xflow_core::{ModelProvider, OpenAIProvider};
use xflow_server::{create_api_router, create_ws_router, AppState};

/// xflow Web API 服务器
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 监听地址
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    addr: String,

    /// API 基础 URL
    /// - vLLM: http://localhost:8000/v1
    /// - OpenAI: https://api.openai.com/v1
    /// - Ollama: http://localhost:11434/v1
    #[arg(short, long, default_value = "http://localhost:11434/v1")]
    base_url: String,

    /// API Key (OpenAI/vLLM 需要，Ollama 可省略)
    #[arg(short = 'k', long, env = "XFLOW_API_KEY")]
    api_key: Option<String>,

    /// 模型名称
    #[arg(short, long, default_value = "qwen3.5-27b")]
    model: String,

    /// 工作目录
    #[arg(short, long, default_value = ".")]
    workdir: String,

    /// 静态文件目录
    #[arg(short, long, default_value = "web")]
    static_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 解析参数
    let args = Args::parse();

    // 初始化日志
    init_logging();

    tracing::info!("启动 xflow Web 服务器");
    tracing::info!("  监听地址：{}", args.addr);
    tracing::info!("  Base URL: {}", args.base_url);
    tracing::info!("  模型：{}", args.model);
    tracing::info!("  工作目录：{}", args.workdir);

    // 创建模型提供者 (OpenAI 兼容模式)
    let provider: Arc<dyn ModelProvider> = Arc::new(OpenAIProvider::new(
        args.base_url,
        args.api_key,
        args.model,
        "openai-compatible".to_string(),
    ));

    // 创建应用状态
    let workdir = PathBuf::from(&args.workdir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&args.workdir));

    let state = Arc::new(AppState::new(provider, workdir));

    // 创建路由
    let app = Router::new()
        // API 路由
        .nest("/api", create_api_router(state.clone()))
        // WebSocket 路由 (使用 XflowEvent 系统)
        .nest("/api", create_ws_router(state.clone()))
        // 健康检查
        .route("/health", get(|| async { "OK" }))
        // 静态文件服务
        .fallback_service(ServeDir::new(&args.static_dir))
        // CORS - 限制允许的来源
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    HeaderValue::from_static("http://localhost:3000"),
                    HeaderValue::from_static("http://127.0.0.1:3000"),
                    HeaderValue::from_static("http://localhost:11434"),  // Ollama 默认端口
                ]))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
        )
        // 日志 - 禁用 ANSI 颜色
        .layer(TraceLayer::new_for_http()
            .make_span_with(|request: &axum::http::Request<_>| {
                tracing::info_span!("HTTP request", method = ?request.method(), uri = ?request.uri())
            }));

    // 启动服务器
    let addr: SocketAddr = args.addr.parse()?;
    tracing::info!("服务器启动：http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// 初始化日志系统
fn init_logging() {
    use chrono::Local;

    // 日志文件路径 - 与 xflow 相同：~/.local/share/xflow/logs
    let log_dir = dirs::data_dir()
        .map(|p| p.join("xflow/logs"))
        .unwrap_or_else(std::env::temp_dir);
    let _ = fs::create_dir_all(&log_dir);

    // 获取当前日期，格式：20260416
    let date_str = Local::now().format("%Y%m%d").to_string();
    let log_file = log_dir.join(format!("xflow-server-{}.log", date_str));

    // 自定义时间格式：2026-04-14 15:11:45.823
    let time_format =
        tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".parse().unwrap());

    // 创建文件日志层 - 禁用 ANSI 颜色
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap()),
        )
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_timer(time_format.clone());

    // 控制台输出层 - 保留 ANSI 颜色
    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(true)
        .with_timer(time_format);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xflow_server=debug,xflow=debug".into()),
        )
        .with(file_layer)
        .with(console_layer)
        .init();
}
