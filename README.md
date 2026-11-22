# Inventory API

[![CI](https://github.com/hannan1985/inventory-api/actions/workflows/ci.yml/badge.svg)](https://github.com/hannan1985/inventory-api/actions/workflows/ci.yml)

A small warehouse inventory service written in Rust. It keeps stock items in
PostgreSQL, records every goods receipt and issue as a stock movement, makes
sure stock can never go below zero, and publishes each movement as an MQTT
event so other warehouse services and devices can react to it.

**Stack:** Rust · axum · tokio · sqlx · PostgreSQL · rumqttc (MQTT) · Mosquitto · Docker Compose · GitHub Actions

## Quick start

You need [Rust](https://rustup.rs) and [Docker Desktop](https://www.docker.com/products/docker-desktop/).

```powershell
git clone https://github.com/hannan1985/inventory-api.git
cd inventory-api
docker compose up -d          # PostgreSQL 17 on 5432, Mosquitto on 1883
copy .env.example .env        # on Linux/macOS: cp .env.example .env
cargo run
```

The server listens on `http://localhost:3000`. Database migrations in
`migrations/` run automatically at startup.

Try it (PowerShell):

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:3000/items -ContentType "application/json" `
  -Body '{"sku":"BOLT-10","name":"Bolt M10","quantity":0}'

Invoke-RestMethod -Method Post -Uri http://localhost:3000/items/BOLT-10/movements -ContentType "application/json" `
  -Body '{"kind":"receive","quantity":100,"note":"PO-1001"}'

Invoke-RestMethod http://localhost:3000/items/BOLT-10
```

Watch the MQTT events in another terminal:

```powershell
docker compose exec mosquitto mosquitto_sub -t "inventory/#" -v
```

## API

| Method | Path | Description | Success | Errors |
|---|---|---|---|---|
| GET | `/health` | Liveness check | 200 | |
| GET | `/version` | Name and version | 200 | |
| GET | `/items` | All items, sorted by SKU | 200 | |
| POST | `/items` | Create an item `{sku, name, quantity?}` | 201 | 400, 409 duplicate SKU |
| GET | `/items/{sku}` | One item | 200 | 404 |
| DELETE | `/items/{sku}` | Delete an item without history | 204 | 404, 409 has movements |
| POST | `/items/{sku}/movements` | Receive or issue stock `{kind, quantity, note?}` | 201 | 400, 404, 409 insufficient stock |
| GET | `/items/{sku}/movements` | Movement history, oldest first | 200 | 404 |

`kind` is `"receive"` or `"issue"`. Every error has the same JSON shape:

```json
{ "error": "Insufficient stock: 70 available, 500 requested" }
```

### MQTT event

Published on `inventory/movements` (QoS 1) after each committed movement:

```json
{
  "event": "stock_movement",
  "movement_id": 42,
  "sku": "BOLT-10",
  "kind": "issue",
  "quantity": 30,
  "new_quantity": 70,
  "at": "2026-11-22T14:27:37.397388Z"
}
```

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `DATABASE_URL` | required | PostgreSQL connection string |
| `BIND_ADDR` | `localhost:3000` | Address the HTTP server listens on |
| `MQTT_HOST` | unset | Broker host; events are disabled when unset |
| `MQTT_PORT` | `1883` | Broker port |
| `RUST_LOG` | `info` | Log level, e.g. `debug` or `inventory_api=debug` |

## Tests

```powershell
docker compose up -d postgres
cargo test
```

`cargo test` runs unit tests for validation and integration tests in
`tests/api.rs` that call the real router against PostgreSQL. `#[sqlx::test]`
creates a fresh database for each test and runs the migrations, so tests are
isolated and can run in parallel. GitHub Actions runs formatting, clippy and
all tests on every push.

## Design decisions

- **Stock never goes negative, even under concurrent requests.** A movement
  runs in one transaction: `SELECT ... FOR UPDATE` locks the item row, the new
  quantity is checked, then the quantity update and the movement record are
  written together. Two requests can't both take the last units. The database
  also enforces `CHECK (quantity >= 0)` as a last line of defence, and a test
  sends 20 parallel issues to prove it.
- **Every change is traceable.** Quantities only change through movements, and
  items with movements can't be deleted (`ON DELETE RESTRICT` → 409).
- **One error type.** `AppError` maps to 400/404/409/500 and always returns
  JSON. Database details are logged on the server but never sent to clients.
- **Events after commit, never blocking.** The MQTT event is queued only after
  the transaction commits, so rolled-back movements are never announced. If
  the broker is down, requests still succeed and a background task reconnects.
- **Plain SQL with sqlx.** Queries are explicit and parameterised, and
  migrations are plain `.sql` files embedded in the binary. Runtime-checked
  queries are used so the project builds without a database.
- **`i32` quantities.** PostgreSQL has no unsigned integers, so the API uses
  `i32` and validates non-negative values both in Rust and in SQL.

## Project structure

```
src/
  main.rs     configuration, database pool, migrations, server start
  lib.rs      AppState and app() so tests can build the same router
  routes.rs   HTTP handlers
  models.rs   request/response types and validation rules
  error.rs    AppError and its HTTP mapping
  events.rs   MQTT publisher
migrations/   SQL schema, applied at startup
tests/api.rs  integration tests
```

## How it was built

The project was built in small steps, one commit each, so the history shows
the progression: first endpoint → in-memory items API → PostgreSQL → stock
movements in transactions → error handling and validation → MQTT events →
tests → CI and documentation.
