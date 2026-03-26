use serde_json::Value;
use sqlx::SqlitePool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::{SubjectResponse, SubjectType},
    error::ApiError,
};

pub async fn write_audit_log(
    pool: &SqlitePool,
    subject: Option<&SubjectResponse>,
    subject_type: Option<SubjectType>,
    identifier: Option<&str>,
    event_type: &str,
    details: Value,
) -> Result<(), ApiError> {
    let created_at = format_timestamp(OffsetDateTime::now_utc())?;
    let subject_type_value = subject
        .map(|subject| subject.subject_type.as_str().to_string())
        .or_else(|| subject_type.map(|value| value.as_str().to_string()));

    sqlx::query(
        r#"
        INSERT INTO audit_logs (id, subject_id, subject_type, identifier, event_type, details_json, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(subject.map(|value| value.id.clone()))
    .bind(subject_type_value)
    .bind(identifier.map(normalize_identifier))
    .bind(event_type)
    .bind(details.to_string())
    .bind(created_at)
    .execute(pool)
    .await
    .map_err(ApiError::from)?;

    Ok(())
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, ApiError> {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| {
            ApiError::internal(
                "TIMESTAMP_FORMAT_FAILED",
                "failed to format server timestamp for audit log",
            )
        })
}

fn normalize_identifier(identifier: &str) -> String {
    let value = identifier.trim();
    if value.contains('@') {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

