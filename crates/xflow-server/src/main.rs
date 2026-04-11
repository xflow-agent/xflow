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
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xflow_model::OllamaProvider;
use xflow_server::{create_api_router, create_ws_router, AppState};

/// xflow Web API 服务器
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 监听地址
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    addr: String,

    /// Ollama 地址
    #[arg(short, long, default_value = "http://localhost:11434")]
    ollama: String,

    /// 模型名称
    #[arg(short, long, default_value = "qwen2.5:7b")]
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
    tracing::info!("  Ollama: {}", args.ollama);
    tracing::info!("  模型: {}", args.model);
    tracing::info!("  工作目录: {}", args.workdir);

    // 创建模型提供者
    let provider = Arc::new(OllamaProvider::new(
        args.ollama,
        args.model,
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
        // CORS
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
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