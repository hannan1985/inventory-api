use inventory_api::{AppState, app, events::EventPublisher};
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

    let events = match std::env::var("MQTT_HOST") {
        Ok(host) => {
            let port = std::env::var("MQTT_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(1883);
            tracing::info!("publishing events to mqtt://{host}:{port}");
            EventPublisher::connect(&host, port)
        }
        Err(_) => {
            tracing::info!("MQTT_HOST not set, events are disabled");
            EventPublisher::disabled()
        }
    };

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server running on http://{bind_addr}");

    axum::serve(listener, app(AppState { pool, events })).await?;
    Ok(())
}
