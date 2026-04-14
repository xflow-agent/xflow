//! xflow Web API 服务器入口

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    routing::get,
    Router,
};
use clap::Parser;
use tower_http::{
    cors::{CorsLayer, AllowOrigin},
    services::ServeDir,
    trace::TraceLayer,
};
use axum::http::{Method, header, HeaderValue};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xflow_model::{ModelProvider, OpenAIProvider};
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
    workdir: String,

    /// 静态文件目录
    #[arg(short, long, default_value = "web")]
    static_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xflow_server=debug,xflow=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 解析参数
    let args = Args::parse();

    tracing::info!("启动 xflow Web 服务器");
    tracing::info!("  监听地址: {}", args.addr);
    tracing::info!("  Base URL: {}", args.base_url);
    tracing::info!("  模型: {}", args.model);
    tracing::info!("  工作目录: {}", args.workdir);

    // 创建模型提供者 (OpenAI 兼容模式)
    let provider: Arc<dyn ModelProvider> = Arc::new(OpenAIProvider::new(
        args.base_url,
        args.api_key,
        args.model,
        "openai-compatible".to_string(),
    ));

    // 创建应用状态
    let workdir = PathBuf::from(&args.workdir).canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&args.workdir));
    
    let state = Arc::new(AppState::new(provider, workdir));

    // 创建路由
    let app = Router::new()
        // API 路由
        .nest("/api", create_api_router(state.clone()))
        // WebSocket 路由
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
        // 日志
        .layer(TraceLayer::new_for_http());

    // 启动服务器
    let addr: SocketAddr = args.addr.parse()?;
    tracing::info!("服务器启动: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}