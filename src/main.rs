use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize)]
struct InventoryApi {
    name: String,
    version: String,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new()
        .route("/health", get(health))
        .route("/version", get(version));

    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .expect("Failed to bind to address localhost:3000");

    println!("Server running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to serve application");
}

async fn version() -> Json<InventoryApi> {
    let inventorya_api: InventoryApi = InventoryApi {
        name: "inventory-api".to_string(),
        version: "1.0.0".to_string(),
    };
    Json(inventorya_api)
}
