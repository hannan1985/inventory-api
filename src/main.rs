use axum::{ routing::get, Json, Router };
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/health", get(health));
    let listener = tokio::net::TcpListener::bind("localhost:3000").await.unwrap();
    println!("Server running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
