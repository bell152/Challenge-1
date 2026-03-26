use moka::future::Cache;
use serde_json::Value;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct RateLimitEntry {
    pub count: u32,
    pub reset_at: String,
}

#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub count: u32,
    pub reset_at: String,
}

pub async fn enforce_rate_limit(
    cache: &Cache<String, RateLimitEntry>,
    key: String,
    max_requests: u32,
    window: Duration,
    code: &str,
    message: &str,
) -> Result<RateLimitStatus, ApiError> {
    let now = OffsetDateTime::now_utc();
    let now_text = format_timestamp(now)?;

    match cache.get(&key).await {
        Some(entry) if entry.reset_at > now_text => {
            if entry.count >= max_requests {
                return Err(ApiError::new(
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    code,
                    format!("{message}. retry after {}", entry.reset_at),
                ));
            }

            let next = RateLimitEntry {
                count: entry.count + 1,
                reset_at: entry.reset_at.clone(),
            };
            cache.insert(key, next.clone()).await;

            Ok(RateLimitStatus {
                count: next.count,
                reset_at: next.reset_at,
            })
        }
        _ => {
            let next = RateLimitEntry {
                count: 1,
                reset_at: format_timestamp(now + window)?,
            };
            cache.insert(key, next.clone()).await;

            Ok(RateLimitStatus {
                count: next.count,
                reset_at: next.reset_at,
            })
        }
    }
}

pub fn identifier_key(namespace: &str, subject_type: &str, identifier: &str) -> String {
    format!("{namespace}:{subject_type}:{}", identifier.trim())
}

pub fn subject_key(namespace: &str, subject_id: &str) -> String {
    format!("{namespace}:{}", subject_id.trim())
}

pub fn details_json(limit: u32, status: &RateLimitStatus) -> Value {
    serde_json::json!({
        "request_count": status.count,
        "limit": limit,
        "reset_at": status.reset_at
    })
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, ApiError> {
    value.format(&Rfc3339).map_err(|_| {
        ApiError::internal(
            "TIMESTAMP_FORMAT_FAILED",
            "failed to format server timestamp for rate limit",
        )
    })
}
