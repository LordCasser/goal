use serde::Serialize;

/// Error returned across the IPC boundary.
///
/// Every variant serializes to `{ "code": string, "message": string }` so the
/// frontend can map a stable `code` to user-facing copy (see `docs/architecture.md`).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Caller-supplied arguments or the current state make the request invalid.
    #[error("{message}")]
    Validation { code: String, message: String },

    /// The addressed entity does not exist.
    #[error("{entity} not found: {id}")]
    NotFound { entity: String, id: String },

    /// The request conflicts with an existing constraint or a deletion guard.
    #[error("{message}")]
    Conflict { code: String, message: String },

    /// Storage-layer failure, passed through with its original message.
    #[error("database error: {0}")]
    Db(String),

    /// Unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Conflict {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn not_found(entity: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity: entity.into(),
            id: id.into(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (code, message) = match self {
            Self::Validation { code, message } => (code.clone(), message.clone()),
            Self::NotFound { entity, id } => {
                ("not_found".to_string(), format!("{entity} not found: {id}"))
            }
            Self::Conflict { code, message } => (code.clone(), message.clone()),
            Self::Db(m) => ("db_error".to_string(), m.clone()),
            Self::Internal(m) => ("internal".to_string(), m.clone()),
        };
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", &code)?;
        s.serialize_field("message", &message)?;
        s.end()
    }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        Self::Db(e.to_string())
    }
}

/// Stable tokens raised by schema triggers via `RAISE(ABORT, '<token>')`.
/// The token becomes the IPC `code` so the frontend can map it to copy
/// (see `docs/architecture.md`, 错误语义).
const TRIGGER_TOKENS: [(&str, &str); 2] = [
    (
        "root_color_key_requires_long_term_cycle",
        "Colors can only be used on goals inside a Long-term cycle.",
    ),
    (
        "cycle_with_root_colors_must_stay_long_term",
        "This cycle contains colored goals and cannot change its type.",
    ),
];

/// Maps a SQLite constraint failure to a `Conflict` with a stable `code`.
/// Unknown constraints degrade to `constraint_violation` and keep the original
/// message so nothing is silently swallowed.
pub fn constraint_conflict(message: &str) -> AppError {
    for (token, copy) in TRIGGER_TOKENS {
        if message.contains(token) {
            return AppError::conflict(token, copy);
        }
    }
    if message.contains("UNIQUE constraint failed") && message.contains("cycles.calendar_key") {
        return AppError::conflict(
            "calendar_key_taken",
            "A cycle already exists for this date; it will be reused.",
        );
    }
    if message.contains("FOREIGN KEY constraint failed") {
        return AppError::conflict(
            "referential_integrity",
            "This operation would leave dangling references.",
        );
    }
    // Unnamed CHECK constraints report only the table name on modern SQLite,
    // so a specific lifecycle code cannot be recovered here. Lifecycle
    // transitions are validated by `domain::cycle::transition` before any SQL
    // runs; the CHECK stays as a storage backstop and surfaces as
    // `constraint_violation`.
    AppError::conflict("constraint_violation", message)
}

/// Converts a raw `rusqlite::Error` into an [`AppError`], mapping constraint
/// violations through [`constraint_conflict`] and everything else to `Db`.
pub fn from_rusqlite(err: rusqlite::Error) -> AppError {
    if let rusqlite::Error::SqliteFailure(ffi_err, msg) = &err {
        if matches!(
            ffi_err.code,
            rusqlite::ffi::ErrorCode::ConstraintViolation
                | rusqlite::ffi::ErrorCode::OperationAborted
        ) {
            let text = msg.clone().unwrap_or_else(|| err.to_string());
            return constraint_conflict(&text);
        }
    }
    AppError::Db(err.to_string())
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_tokens_map_to_conflict_with_token_code() {
        for (token, _) in TRIGGER_TOKENS {
            let err = constraint_conflict(token);
            match err {
                AppError::Conflict { code, .. } => assert_eq!(code, token),
                other => panic!("expected Conflict, got {other:?}"),
            }
        }
    }

    #[test]
    fn calendar_key_unique_maps_to_taken() {
        let err = constraint_conflict("UNIQUE constraint failed: cycles.calendar_key");
        match err {
            AppError::Conflict { code, .. } => assert_eq!(code, "calendar_key_taken"),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn unknown_constraint_degrades_with_original_message() {
        let err = constraint_conflict("CHECK constraint failed: cycles");
        match err {
            AppError::Conflict { code, message } => {
                assert_eq!(code, "constraint_violation");
                assert!(message.contains("CHECK"));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn error_serializes_to_code_and_message() {
        let err = AppError::not_found("cycle", "abc");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "not_found");
        assert!(json["message"].as_str().unwrap().contains("abc"));
    }
}
