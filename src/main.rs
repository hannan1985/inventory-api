use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize)]
struct InventoryApi {
    name: String,
    version: String,
}

#[derive(Clone, Serialize, Deserialize, FromRow)]
struct Item {
    sku: String,
    name: String,
    quantity: i32,
}

type AppState = PgPool;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    let app: Router = Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/items", get(list_items).post(create_item))
        .route("/items/{sku}", get(get_item).delete(delete_item))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .expect("Failed to bind to address localhost:3000");

    println!("Server running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to serve application");
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn version() -> Json<InventoryApi> {
    Json(InventoryApi {
        name: "inventory-api".to_string(),
        version: "0.1.0".to_string(),
    })
}

// POST /items
async fn create_item(
    State(pool): State<AppState>,
    Json(mut item): Json<Item>,
) -> Result<(StatusCode, Json<Item>), (StatusCode, String)> {
    item.sku = item.sku.trim().to_string();
    item.name = item.name.trim().to_string();

    if item.sku.is_empty() || item.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "SKU and name are required".to_string(),
        ));
    }
    if item.quantity < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Quantity cannot be negative".to_string(),
        ));
    }

    let result = sqlx::query_as::<_, Item>(
        "INSERT INTO items (sku, name, quantity) VALUES ($1, $2, $3)
         RETURNING sku, name, quantity",
    )
    .bind(&item.sku)
    .bind(&item.name)
    .bind(item.quantity)
    .fetch_one(&pool)
    .await;

    match result {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err((
            StatusCode::CONFLICT,
            "An item with this SKU already exists".to_string(),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )),
    }
}

// GET /items
async fn list_items(State(pool): State<AppState>) -> Result<Json<Vec<Item>>, StatusCode> {
    let items = sqlx::query_as::<_, Item>("SELECT sku, name, quantity FROM items ORDER BY sku")
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(items))
}

// GET /items/{sku}
async fn get_item(
    State(pool): State<AppState>,
    Path(sku): Path<String>,
) -> Result<Json<Item>, StatusCode> {
    sqlx::query_as::<_, Item>("SELECT sku, name, quantity FROM items WHERE sku = $1")
        .bind(&sku)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// DELETE /items/{sku}
async fn delete_item(State(pool): State<AppState>, Path(sku): Path<String>) -> StatusCode {
    match sqlx::query("DELETE FROM items WHERE sku = $1")
        .bind(&sku)
        .execute(&pool)
        .await
    {
        Ok(result) if result.rows_affected() == 1 => StatusCode::NO_CONTENT,
        Ok(_) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
