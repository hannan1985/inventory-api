//! Publishes inventory events to an MQTT broker.
//!
//! The API must keep working if the broker is down, so publishing never
//! blocks or fails a request: events are queued with `try_publish` and a
//! background task keeps the connection alive and reconnects.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::Serialize;

use crate::models::{Movement, MovementKind};

pub const MOVEMENTS_TOPIC: &str = "inventory/movements";

#[derive(Debug, Serialize)]
pub struct MovementEvent<'a> {
    pub event: &'static str,
    pub movement_id: i64,
    pub sku: &'a str,
    pub kind: MovementKind,
    pub quantity: i32,
    pub new_quantity: i32,
    pub at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct EventPublisher {
    client: Option<AsyncClient>,
}

impl EventPublisher {
    /// No broker configured (or in tests): events are simply skipped.
    pub fn disabled() -> Self {
        Self { client: None }
    }

    /// Connects to `host:port` and spawns the MQTT event loop on tokio.
    pub fn connect(host: &str, port: u16) -> Self {
        let mut options = MqttOptions::new("inventory-api", host, port);
        options.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = AsyncClient::new(options, 100);

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!("connected to MQTT broker");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "MQTT connection error, retrying in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Self {
            client: Some(client),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.client.is_some()
    }

    pub fn movement_recorded(&self, movement: &Movement, new_quantity: i32) {
        let Some(client) = &self.client else {
            return;
        };

        let event = MovementEvent {
            event: "stock_movement",
            movement_id: movement.id,
            sku: &movement.sku,
            kind: movement.kind,
            quantity: movement.quantity,
            new_quantity,
            at: movement.created_at,
        };

        match serde_json::to_vec(&event) {
            Ok(payload) => {
                if let Err(e) =
                    client.try_publish(MOVEMENTS_TOPIC, QoS::AtLeastOnce, false, payload)
                {
                    tracing::warn!(error = %e, "could not queue MQTT event");
                }
            }
            Err(e) => tracing::error!(error = %e, "could not serialise MQTT event"),
        }
    }
}
