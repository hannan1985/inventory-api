use inventory_api::{AppState, app};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set (see .env.example)")?;
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "localhost:3000".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server running on http://{bind_addr}");

    axum::serve(listener, app(AppState { pool })).await?;
    Ok(())
}
