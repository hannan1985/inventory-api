use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex},
};

use axum::http::StatusCode;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize)]
struct InventoryApi {
    name: String,
    version: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Item {
    sku: String,
    name: String,
    quantity: u32,
}

type AppState = Arc<Mutex<HashMap<String, Item>>>;

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(HashMap::new()));

    let app: Router = Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/items", get(list_items).post(create_item))
        .route("/items/{sku}", get(get_item))
        .with_state(state);

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
        version: "0.1.0".to_string(),
    };
    Json(inventorya_api)
}

// POST /items
async fn create_item(
    State(state): State<AppState>,
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

    let mut items = state.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not access inventory".to_string(),
        )
    })?;

    match items.entry(item.sku.clone()) {
        Entry::Occupied(_) => Err((
            StatusCode::CONFLICT,
            "An item with this SKU already exists".to_string(),
        )),
        Entry::Vacant(entry) => {
            entry.insert(item.clone());
            Ok((StatusCode::CREATED, Json(item)))
        }
    }
}

// GET /items
async fn list_items(State(state): State<AppState>) -> Result<Json<Vec<Item>>, StatusCode> {
    let items = state
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result: Vec<Item> = items.values().cloned().collect();
    result.sort_by(|a, b| a.sku.cmp(&b.sku));

    Ok(Json(result))
}

// GET /items/{sku}
async fn get_item(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> Result<Json<Item>, StatusCode> {
    let items = state
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    items
        .get(&sku)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
