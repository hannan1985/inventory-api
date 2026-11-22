use axum::{
    Json, Router,
    extract::{FromRequest, Path, State},
    http::StatusCode,
    routing::get,
};
use serde::Serialize;

use crate::{
    AppState,
    error::{AppError, AppResult},
    models::{Item, Movement, MovementKind, MovementResult, NewItem, NewMovement},
};

/// Same as axum's `Json`, but a bad body becomes our `AppError` (400 + JSON).
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct AppJson<T>(pub T);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/items", get(list_items).post(create_item))
        .route("/items/{sku}", get(get_item).delete(delete_item))
        .route(
            "/items/{sku}/movements",
            get(list_movements).post(create_movement),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize)]
struct InventoryApi {
    name: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn version() -> Json<InventoryApi> {
    Json(InventoryApi {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    })
}

fn item_not_found(sku: &str) -> AppError {
    AppError::NotFound(format!("Item '{sku}' not found"))
}

// POST /items
async fn create_item(
    State(state): State<AppState>,
    AppJson(input): AppJson<NewItem>,
) -> AppResult<(StatusCode, Json<Item>)> {
    let item = input.validate()?;

    let created = sqlx::query_as::<_, Item>(
        "INSERT INTO items (sku, name, quantity) VALUES ($1, $2, $3)
         RETURNING sku, name, quantity",
    )
    .bind(&item.sku)
    .bind(&item.name)
    .bind(item.quantity)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict(format!("An item with SKU '{}' already exists", item.sku))
        }
        other => AppError::from(other),
    })?;

    Ok((StatusCode::CREATED, Json(created)))
}

// GET /items
async fn list_items(State(state): State<AppState>) -> AppResult<Json<Vec<Item>>> {
    let items = sqlx::query_as::<_, Item>("SELECT sku, name, quantity FROM items ORDER BY sku")
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(items))
}

// GET /items/{sku}
async fn get_item(State(state): State<AppState>, Path(sku): Path<String>) -> AppResult<Json<Item>> {
    sqlx::query_as::<_, Item>("SELECT sku, name, quantity FROM items WHERE sku = $1")
        .bind(&sku)
        .fetch_optional(&state.pool)
        .await?
        .map(Json)
        .ok_or_else(|| item_not_found(&sku))
}

// DELETE /items/{sku}
async fn delete_item(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> AppResult<StatusCode> {
    let result = sqlx::query("DELETE FROM items WHERE sku = $1")
        .bind(&sku)
        .execute(&state.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => AppError::Conflict(
                format!("Item '{sku}' has stock movements and cannot be deleted"),
            ),
            other => AppError::from(other),
        })?;

    if result.rows_affected() == 0 {
        return Err(item_not_found(&sku));
    }
    Ok(StatusCode::NO_CONTENT)
}

// POST /items/{sku}/movements
// The quantity change and the movement record are written in one transaction,
// and the item row is locked so two requests can't both take the last units.
async fn create_movement(
    State(state): State<AppState>,
    Path(sku): Path<String>,
    AppJson(input): AppJson<NewMovement>,
) -> AppResult<(StatusCode, Json<MovementResult>)> {
    let input = input.validate()?;

    let mut tx = state.pool.begin().await?;

    let current: i32 = sqlx::query_scalar("SELECT quantity FROM items WHERE sku = $1 FOR UPDATE")
        .bind(&sku)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| item_not_found(&sku))?;

    let new_quantity = match input.kind {
        MovementKind::Receive => current
            .checked_add(input.quantity)
            .ok_or_else(|| AppError::BadRequest("Quantity too large".into()))?,
        MovementKind::Issue => current - input.quantity,
    };

    if new_quantity < 0 {
        // Dropping `tx` here rolls the transaction back.
        return Err(AppError::Conflict(format!(
            "Insufficient stock: {current} available, {} requested",
            input.quantity
        )));
    }

    sqlx::query("UPDATE items SET quantity = $1 WHERE sku = $2")
        .bind(new_quantity)
        .bind(&sku)
        .execute(&mut *tx)
        .await?;

    let movement = sqlx::query_as::<_, Movement>(
        "INSERT INTO stock_movements (sku, kind, quantity, note)
         VALUES ($1, $2, $3, $4)
         RETURNING id, sku, kind, quantity, note, created_at",
    )
    .bind(&sku)
    .bind(input.kind)
    .bind(input.quantity)
    .bind(&input.note)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(sku = %sku, kind = ?movement.kind, quantity = movement.quantity, new_quantity, "stock movement");

    Ok((
        StatusCode::CREATED,
        Json(MovementResult {
            movement,
            new_quantity,
        }),
    ))
}

// GET /items/{sku}/movements
async fn list_movements(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> AppResult<Json<Vec<Movement>>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT sku FROM items WHERE sku = $1")
        .bind(&sku)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_none() {
        return Err(item_not_found(&sku));
    }

    let movements = sqlx::query_as::<_, Movement>(
        "SELECT id, sku, kind, quantity, note, created_at
         FROM stock_movements WHERE sku = $1 ORDER BY created_at, id",
    )
    .bind(&sku)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(movements))
}
