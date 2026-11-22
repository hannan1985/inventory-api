use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Item {
    pub sku: String,
    pub name: String,
    pub quantity: i32,
}

#[derive(Debug, Deserialize)]
pub struct NewItem {
    pub sku: String,
    pub name: String,
    #[serde(default)]
    pub quantity: i32,
}

impl NewItem {
    /// Trims input and checks the business rules. Returns a cleaned copy.
    pub fn validate(self) -> Result<NewItem, AppError> {
        let sku = self.sku.trim().to_string();
        let name = self.name.trim().to_string();

        if sku.is_empty() || name.is_empty() {
            return Err(AppError::BadRequest("SKU and name are required".into()));
        }
        if sku.len() > 64 {
            return Err(AppError::BadRequest(
                "SKU can be at most 64 characters".into(),
            ));
        }
        if !sku
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::BadRequest(
                "SKU may only contain letters, digits, '-' and '_'".into(),
            ));
        }
        if name.chars().count() > 200 {
            return Err(AppError::BadRequest(
                "Name can be at most 200 characters".into(),
            ));
        }
        if self.quantity < 0 {
            return Err(AppError::BadRequest("Quantity cannot be negative".into()));
        }

        Ok(NewItem {
            sku,
            name,
            quantity: self.quantity,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum MovementKind {
    Receive,
    Issue,
}

#[derive(Debug, Deserialize)]
pub struct NewMovement {
    pub kind: MovementKind,
    pub quantity: i32,
    pub note: Option<String>,
}

impl NewMovement {
    pub fn validate(self) -> Result<NewMovement, AppError> {
        if self.quantity <= 0 {
            return Err(AppError::BadRequest(
                "Quantity must be greater than zero".into(),
            ));
        }
        let note = self
            .note
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());
        if note.as_ref().is_some_and(|n| n.chars().count() > 500) {
            return Err(AppError::BadRequest(
                "Note can be at most 500 characters".into(),
            ));
        }
        Ok(NewMovement { note, ..self })
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Movement {
    pub id: i64,
    pub sku: String,
    pub kind: MovementKind,
    pub quantity: i32,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MovementResult {
    pub movement: Movement,
    pub new_quantity: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(sku: &str, name: &str, quantity: i32) -> NewItem {
        NewItem {
            sku: sku.into(),
            name: name.into(),
            quantity,
        }
    }

    #[test]
    fn valid_item_is_trimmed() {
        let item = item("  ABC-1 ", " Widget ", 3).validate().unwrap();
        assert_eq!(item.sku, "ABC-1");
        assert_eq!(item.name, "Widget");
    }

    #[test]
    fn rejects_bad_items() {
        assert!(item("", "x", 0).validate().is_err());
        assert!(item("A", "   ", 0).validate().is_err());
        assert!(item("A B", "x", 0).validate().is_err());
        assert!(item(&"A".repeat(65), "x", 0).validate().is_err());
        assert!(item("A", "x", -1).validate().is_err());
    }

    #[test]
    fn movement_quantity_must_be_positive() {
        let m = |q| NewMovement {
            kind: MovementKind::Issue,
            quantity: q,
            note: None,
        };
        assert!(m(0).validate().is_err());
        assert!(m(-1).validate().is_err());
        assert!(m(1).validate().is_ok());
    }

    #[test]
    fn blank_note_becomes_none() {
        let m = NewMovement {
            kind: MovementKind::Receive,
            quantity: 1,
            note: Some("   ".into()),
        }
        .validate()
        .unwrap();
        assert_eq!(m.note, None);
    }
}
