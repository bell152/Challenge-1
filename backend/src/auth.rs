use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
};
use axum::{
    Json,
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    AppState, SharedState, audit::write_audit_log, error::ApiError,
    rate_limit::{details_json as rate_limit_details_json, enforce_rate_limit, identifier_key},
};

const ACCESS_TOKEN_TTL_MINUTES: i64 = 15;
const REFRESH_TOKEN_TTL_DAYS: i64 = 7;
const OTP_TTL_MINUTES: i64 = 5;
const OTP_MAX_ATTEMPTS: u8 = 5;
const OTP_REQUEST_RATE_LIMIT: u32 = 3;
const OTP_REQUEST_RATE_LIMIT_WINDOW_MINUTES: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubjectType {
    Member,
    CommunityStaff,
    PlatformStaff,
}

impl SubjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Member => "MEMBER",
            Self::CommunityStaff => "COMMUNITY_STAFF",
            Self::PlatformStaff => "PLATFORM_STAFF",
        }
    }
}

impl std::str::FromStr for SubjectType {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "MEMBER" => Ok(Self::Member),
            "COMMUNITY_STAFF" => Ok(Self::CommunityStaff),
            "PLATFORM_STAFF" => Ok(Self::PlatformStaff),
            _ => Err(ApiError::bad_request(
                "INVALID_SUBJECT_TYPE",
                "subject_type must be one of MEMBER, COMMUNITY_STAFF, PLATFORM_STAFF",
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PasswordLoginRequest {
    pub subject_type: String,
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct OtpRequestPayload {
    pub subject_type: String,
    pub identifier: String,
}

#[derive(Debug, Deserialize)]
pub struct OtpVerifyPayload {
    pub challenge_id: String,
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub subject: SubjectResponse,
    pub session: SessionResponse,
}

#[derive(Debug, Serialize)]
pub struct OtpRequestResponse {
    pub challenge_id: String,
    pub channel_type: String,
    pub masked_destination: String,
    pub expires_at: String,
    pub max_attempts: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub subject: SubjectResponse,
    pub session: SessionResponse,
}

#[derive(Debug, Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionResponse>,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectResponse {
    pub id: String,
    pub subject_type: SubjectType,
    pub display_name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub device_id: String,
    pub device_label: String,
    pub user_agent: String,
    pub login_method: String,
    pub status: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct OtpChallengeEntry {
    subject: SubjectResponse,
    channel_value: String,
    code: String,
    expires_at: String,
    attempts_left: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    session_id: String,
    subject_type: SubjectType,
    exp: i64,
    iat: i64,
}

#[derive(Debug)]
struct LoginSubjectRecord {
    subject: SubjectResponse,
    password_hash: String,
}

#[derive(Debug)]
struct OtpIdentityRecord {
    subject: SubjectResponse,
    channel_type: String,
    channel_value: String,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    session_id: String,
    device_id: String,
    device_label: String,
    user_agent: String,
    login_method: String,
    status: String,
    created_at: String,
    last_seen_at: String,
    expires_at: String,
}

#[derive(Debug)]
struct SessionContext {
    subject: SubjectResponse,
    session: SessionRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthenticatedSession {
    pub subject: SubjectResponse,
    pub session: SessionResponse,
}

pub async fn password_login(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<PasswordLoginRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<AuthResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with subject_type, identifier, and password",
        )
    })?;

    let subject_type: SubjectType = request.subject_type.parse()?;
    let identifier = normalize_identifier(&request.identifier);

    if identifier.is_empty() || request.password.trim().is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "identifier and password are required",
        ));
    }

    let record = match find_subject_by_password(&state.db, subject_type, &identifier).await {
        Ok(record) => record,
        Err(error) => {
            let _ = write_audit_log(
                &state.db,
                None,
                Some(subject_type),
                Some(&identifier),
                "LOGIN_FAILED",
                json!({"method":"PASSWORD","reason":"SUBJECT_NOT_FOUND"}),
            )
            .await;
            return Err(error);
        }
    };

    if let Err(error) = verify_password(&request.password, &record.password_hash) {
        let _ = write_audit_log(
            &state.db,
            Some(&record.subject),
            Some(record.subject.subject_type),
            Some(&identifier),
            "LOGIN_FAILED",
            json!({"method":"PASSWORD","reason":"INVALID_PASSWORD"}),
        )
        .await;
        return Err(error);
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("Unknown device");

    let response = create_session_and_tokens(&state, record.subject.clone(), user_agent, "PASSWORD")
        .await?;
    let _ = write_audit_log(
        &state.db,
        Some(&record.subject),
        Some(record.subject.subject_type),
        Some(&identifier),
        "LOGIN_SUCCESS",
        json!({"method":"PASSWORD","session_id":response.session.session_id,"device_id":response.session.device_id}),
    )
    .await;

    Ok(Json(response))
}

pub async fn otp_request(
    State(state): State<SharedState>,
    payload: Result<Json<OtpRequestPayload>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OtpRequestResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with subject_type and identifier",
        )
    })?;

    let subject_type: SubjectType = request.subject_type.parse()?;
    let identifier = normalize_identifier(&request.identifier);

    if identifier.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "identifier is required",
        ));
    }

    let rate_limit_key = identifier_key("otp-request", subject_type.as_str(), &identifier);
    let rate_limit_status = match enforce_rate_limit(
        &state.rate_limit_cache,
        rate_limit_key,
        OTP_REQUEST_RATE_LIMIT,
        Duration::minutes(OTP_REQUEST_RATE_LIMIT_WINDOW_MINUTES),
        "OTP_REQUEST_RATE_LIMITED",
        "otp request rate limit exceeded",
    )
    .await
    {
        Ok(status) => status,
        Err(error) => {
            let _ = write_audit_log(
                &state.db,
                None,
                Some(subject_type),
                Some(&identifier),
                "OTP_REQUEST_RATE_LIMITED",
                serde_json::json!({
                    "limit": OTP_REQUEST_RATE_LIMIT,
                    "window_minutes": OTP_REQUEST_RATE_LIMIT_WINDOW_MINUTES
                }),
            )
            .await;
            return Err(error);
        }
    };

    let identity = match find_subject_by_otp_identity(&state.db, subject_type, &identifier).await {
        Ok(identity) => identity,
        Err(error) => {
            let _ = write_audit_log(
                &state.db,
                None,
                Some(subject_type),
                Some(&identifier),
                "OTP_REQUEST_FAILED",
                json!({"reason":"OTP_IDENTITY_NOT_FOUND"}),
            )
            .await;
            return Err(error);
        }
    };

    let expires_at = OffsetDateTime::now_utc() + Duration::minutes(OTP_TTL_MINUTES);
    let expires_at_text = format_timestamp(expires_at)?;
    let challenge_id = Uuid::new_v4().to_string();
    let code = generate_otp_code(&challenge_id, &identity.channel_value);

    state
        .otp_cache
        .insert(
            challenge_id.clone(),
            OtpChallengeEntry {
                subject: identity.subject.clone(),
                channel_value: identity.channel_value.clone(),
                code: code.clone(),
                expires_at: expires_at_text.clone(),
                attempts_left: OTP_MAX_ATTEMPTS,
            },
        )
        .await;

    let _ = write_audit_log(
        &state.db,
        Some(&identity.subject),
        Some(identity.subject.subject_type),
        Some(&identifier),
        "OTP_REQUESTED",
        json!({
            "channel_type": identity.channel_type,
            "masked_destination": mask_delivery_target(&identity.channel_value),
            "challenge_id": challenge_id,
            "rate_limit": rate_limit_details_json(OTP_REQUEST_RATE_LIMIT, &rate_limit_status)
        }),
    )
    .await;

    Ok(Json(OtpRequestResponse {
        challenge_id,
        channel_type: identity.channel_type,
        masked_destination: mask_delivery_target(&identity.channel_value),
        expires_at: expires_at_text,
        max_attempts: OTP_MAX_ATTEMPTS,
        dev_code: state.is_dev_mode.then_some(code),
    }))
}

pub async fn otp_verify(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<OtpVerifyPayload>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<AuthResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with challenge_id and code",
        )
    })?;

    if request.challenge_id.trim().is_empty() || request.code.trim().is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "challenge_id and code are required",
        ));
    }

    let challenge_id = request.challenge_id.trim().to_string();
    let challenge = state
        .otp_cache
        .get(&challenge_id)
        .await
        .ok_or_else(|| {
            ApiError::unauthorized(
                "OTP_CHALLENGE_NOT_FOUND",
                "otp challenge is invalid, expired, or already used",
            )
        })?;

    let now_text = format_timestamp(OffsetDateTime::now_utc())?;
    if challenge.expires_at <= now_text {
        state.otp_cache.invalidate(&challenge_id).await;
        let _ = write_audit_log(
            &state.db,
            Some(&challenge.subject),
            Some(challenge.subject.subject_type),
            Some(&challenge.channel_value),
            "OTP_VERIFY_FAILED",
            json!({"reason":"OTP_EXPIRED","challenge_id":challenge_id}),
        )
        .await;
        return Err(ApiError::unauthorized(
            "OTP_EXPIRED",
            "otp code has expired, please request a new code",
        ));
    }

    if challenge.code != request.code.trim() {
        let remaining_attempts = challenge.attempts_left.saturating_sub(1);

        if remaining_attempts == 0 {
            state.otp_cache.invalidate(&challenge_id).await;
            let _ = write_audit_log(
                &state.db,
                Some(&challenge.subject),
                Some(challenge.subject.subject_type),
                Some(&challenge.channel_value),
                "OTP_VERIFY_FAILED",
                json!({"reason":"MAX_ATTEMPTS_EXCEEDED","challenge_id":challenge_id}),
            )
            .await;
            return Err(ApiError::bad_request(
                "OTP_MAX_ATTEMPTS_EXCEEDED",
                "otp maximum attempt count exceeded, please request a new code",
            ));
        }

        state
            .otp_cache
            .insert(
                challenge_id.clone(),
                OtpChallengeEntry {
                    attempts_left: remaining_attempts,
                    ..challenge.clone()
                },
            )
            .await;
        let _ = write_audit_log(
            &state.db,
            Some(&challenge.subject),
            Some(challenge.subject.subject_type),
            Some(&challenge.channel_value),
            "OTP_VERIFY_FAILED",
            json!({"reason":"INVALID_CODE","challenge_id":challenge_id,"attempts_left":remaining_attempts}),
        )
        .await;
        return Err(ApiError::unauthorized(
            "INVALID_OTP_CODE",
            "otp code is incorrect",
        ));
    }

    state.otp_cache.invalidate(&challenge_id).await;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("Unknown device");
    let response = create_session_and_tokens(&state, challenge.subject.clone(), user_agent, "OTP")
        .await?;

    let _ = write_audit_log(
        &state.db,
        Some(&challenge.subject),
        Some(challenge.subject.subject_type),
        Some(&challenge.channel_value),
        "OTP_VERIFIED",
        json!({"challenge_id":challenge_id,"session_id":response.session.session_id}),
    )
    .await;
    let _ = write_audit_log(
        &state.db,
        Some(&challenge.subject),
        Some(challenge.subject.subject_type),
        Some(&challenge.channel_value),
        "LOGIN_SUCCESS",
        json!({"method":"OTP","session_id":response.session.session_id,"device_id":response.session.device_id}),
    )
    .await;

    Ok(Json(response))
}

pub async fn refresh(
    State(state): State<SharedState>,
    payload: Result<Json<RefreshRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<AuthResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with refresh_token",
        )
    })?;

    if request.refresh_token.trim().is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "refresh_token is required",
        ));
    }

    let now = OffsetDateTime::now_utc();
    let now_text = format_timestamp(now)?;
    let refresh_hash = hash_refresh_token(request.refresh_token.trim());
    let context = find_active_session_by_refresh_hash(&state.db, &refresh_hash, &now_text).await?;
    let next_refresh_token = generate_refresh_token();
    let next_refresh_hash = hash_refresh_token(&next_refresh_token);

    let mut transaction = state.db.begin().await.map_err(ApiError::from)?;

    sqlx::query(
        r#"
        UPDATE sessions
        SET refresh_token_hash = ?1,
            last_seen_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(&next_refresh_hash)
    .bind(&now_text)
    .bind(&context.session.session_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::from)?;

    sqlx::query(
        r#"
        UPDATE devices
        SET last_seen_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(&now_text)
    .bind(&context.session.device_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::from)?;

    transaction.commit().await.map_err(ApiError::from)?;

    let session = SessionRecord {
        last_seen_at: now_text.clone(),
        ..context.session
    };
    let access_token = create_access_token(&state, &context.subject, &session.session_id, now)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token: next_refresh_token,
        subject: context.subject,
        session: session_record_to_response(&session, &session.session_id),
    }))
}

pub async fn logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, ApiError> {
    let context = authenticate_request(&state, &headers).await?;
    let now_text = format_timestamp(OffsetDateTime::now_utc())?;

    sqlx::query(
        r#"
        UPDATE sessions
        SET status = 'LOGGED_OUT',
            last_seen_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(&now_text)
    .bind(&context.session.session_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ActionResponse {
        success: true,
        message: "current session logged out".to_string(),
        session_id: Some(context.session.session_id),
        revoked_count: None,
    }))
}

pub async fn logout_all(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, ApiError> {
    let context = authenticate_request(&state, &headers).await?;
    let now_text = format_timestamp(OffsetDateTime::now_utc())?;

    let result = sqlx::query(
        r#"
        UPDATE sessions
        SET status = 'LOGGED_OUT',
            last_seen_at = ?1
        WHERE subject_id = ?2
          AND status = 'ACTIVE'
        "#,
    )
    .bind(&now_text)
    .bind(&context.subject.id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ActionResponse {
        success: true,
        message: "all active sessions logged out".to_string(),
        session_id: None,
        revoked_count: Some(result.rows_affected()),
    }))
}

pub async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let context = authenticate_request(&state, &headers).await?;

    Ok(Json(MeResponse {
        subject: context.subject,
        session: session_record_to_response(&context.session, &context.session.session_id),
    }))
}

pub async fn list_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<SessionsResponse>, ApiError> {
    let context = authenticate_request(&state, &headers).await?;
    let sessions =
        load_subject_sessions(&state.db, &context.subject.id, &context.session.session_id).await?;

    Ok(Json(SessionsResponse { sessions }))
}

pub async fn revoke_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ActionResponse>, ApiError> {
    let context = authenticate_request(&state, &headers).await?;
    let now_text = format_timestamp(OffsetDateTime::now_utc())?;

    let row = sqlx::query(
        r#"
        SELECT status
        FROM sessions
        WHERE id = ?1
          AND subject_id = ?2
        LIMIT 1
        "#,
    )
    .bind(&session_id)
    .bind(&context.subject.id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::not_found(
            "SESSION_NOT_FOUND",
            "session was not found for current subject",
        ));
    };

    let status: String = row.get("status");
    if status != "ACTIVE" {
        return Err(ApiError::bad_request(
            "SESSION_NOT_ACTIVE",
            "only active sessions can be revoked",
        ));
    }

    sqlx::query(
        r#"
        UPDATE sessions
        SET status = 'REVOKED',
            last_seen_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(&now_text)
    .bind(&session_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ActionResponse {
        success: true,
        message: if session_id == context.session.session_id {
            "current session revoked"
        } else {
            "session revoked"
        }
        .to_string(),
        session_id: Some(session_id),
        revoked_count: None,
    }))
}

pub async fn authenticate_bearer(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedSession, ApiError> {
    let context = authenticate_request(state, headers).await?;
    let current_session_id = context.session.session_id.clone();

    Ok(AuthenticatedSession {
        subject: context.subject,
        session: session_record_to_response(&context.session, &current_session_id),
    })
}

pub(crate) async fn create_session_and_tokens(
    state: &AppState,
    subject: SubjectResponse,
    user_agent: &str,
    login_method: &str,
) -> Result<AuthResponse, ApiError> {
    let now = OffsetDateTime::now_utc();
    let now_text = format_timestamp(now)?;
    let session_expires_at = format_timestamp(now + Duration::days(REFRESH_TOKEN_TTL_DAYS))?;

    let device_id = Uuid::new_v4().to_string();
    let session_id = Uuid::new_v4().to_string();
    let refresh_token = generate_refresh_token();
    let refresh_token_hash = hash_refresh_token(&refresh_token);
    let device_label = device_label_from_user_agent(user_agent);

    let mut transaction = state.db.begin().await.map_err(ApiError::from)?;

    sqlx::query(
        r#"
        INSERT INTO devices (id, subject_id, label, user_agent, created_at, last_seen_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&device_id)
    .bind(&subject.id)
    .bind(&device_label)
    .bind(user_agent)
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::from)?;

    sqlx::query(
        r#"
        INSERT INTO sessions (
            id, subject_id, device_id, refresh_token_hash, login_method, status, created_at, expires_at, last_seen_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'ACTIVE', ?6, ?7, ?6)
        "#,
    )
    .bind(&session_id)
    .bind(&subject.id)
    .bind(&device_id)
    .bind(&refresh_token_hash)
    .bind(login_method)
    .bind(&now_text)
    .bind(&session_expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::from)?;

    transaction.commit().await.map_err(ApiError::from)?;

    let access_token = create_access_token(state, &subject, &session_id, now)?;
    let session = SessionRecord {
        session_id: session_id.clone(),
        device_id,
        device_label,
        user_agent: user_agent.to_string(),
        login_method: login_method.to_string(),
        status: "ACTIVE".to_string(),
        created_at: now_text.clone(),
        last_seen_at: now_text,
        expires_at: session_expires_at,
    };

    Ok(AuthResponse {
        access_token,
        refresh_token,
        subject,
        session: session_record_to_response(&session, &session_id),
    })
}

async fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SessionContext, ApiError> {
    let token = extract_bearer_token(headers)?;
    let claims = decode_access_token(state, token)?;
    let now_text = format_timestamp(OffsetDateTime::now_utc())?;
    let context =
        find_active_session_by_id(&state.db, &claims.sub, &claims.session_id, &now_text).await?;

    if context.subject.subject_type != claims.subject_type {
        return Err(ApiError::unauthorized(
            "INVALID_ACCESS_TOKEN",
            "access token subject does not match active session",
        ));
    }

    Ok(context)
}

async fn find_subject_by_password(
    pool: &SqlitePool,
    subject_type: SubjectType,
    identifier: &str,
) -> Result<LoginSubjectRecord, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name,
            pc.password_hash
        FROM subjects s
        INNER JOIN subject_identifiers si ON si.subject_id = s.id
        INNER JOIN password_credentials pc ON pc.subject_id = s.id
        WHERE s.subject_type = ?1
          AND s.status = 'ACTIVE'
          AND si.identifier_value = ?2
        LIMIT 1
        "#,
    )
    .bind(subject_type.as_str())
    .bind(identifier)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized(
            "INVALID_CREDENTIALS",
            "identifier or password is incorrect",
        ));
    };

    Ok(LoginSubjectRecord {
        subject: map_subject_row(&row),
        password_hash: row.get("password_hash"),
    })
}

async fn find_subject_by_otp_identity(
    pool: &SqlitePool,
    subject_type: SubjectType,
    identifier: &str,
) -> Result<OtpIdentityRecord, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name,
            oi.channel_type,
            oi.channel_value
        FROM subjects s
        INNER JOIN otp_identities oi ON oi.subject_id = s.id
        WHERE s.subject_type = ?1
          AND s.status = 'ACTIVE'
          AND oi.is_enabled = 1
          AND oi.channel_value = ?2
        LIMIT 1
        "#,
    )
    .bind(subject_type.as_str())
    .bind(identifier)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized(
            "OTP_IDENTITY_NOT_FOUND",
            "identifier is not enabled for otp login",
        ));
    };

    Ok(OtpIdentityRecord {
        subject: map_subject_row(&row),
        channel_type: row.get("channel_type"),
        channel_value: row.get("channel_value"),
    })
}

async fn find_active_session_by_id(
    pool: &SqlitePool,
    subject_id: &str,
    session_id: &str,
    now_text: &str,
) -> Result<SessionContext, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name,
            sess.id AS session_id,
            sess.device_id,
            d.label AS device_label,
            d.user_agent,
            sess.login_method,
            sess.status AS session_status,
            sess.created_at,
            sess.last_seen_at,
            sess.expires_at
        FROM sessions sess
        INNER JOIN subjects s ON s.id = sess.subject_id
        INNER JOIN devices d ON d.id = sess.device_id
        WHERE sess.id = ?1
          AND sess.subject_id = ?2
          AND sess.status = 'ACTIVE'
          AND sess.expires_at > ?3
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .bind(subject_id)
    .bind(now_text)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized(
            "SESSION_NOT_FOUND",
            "active session was not found",
        ));
    };

    Ok(SessionContext {
        subject: map_subject_row(&row),
        session: map_session_row(&row),
    })
}

async fn find_active_session_by_refresh_hash(
    pool: &SqlitePool,
    refresh_token_hash: &str,
    now_text: &str,
) -> Result<SessionContext, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name,
            sess.id AS session_id,
            sess.device_id,
            d.label AS device_label,
            d.user_agent,
            sess.login_method,
            sess.status AS session_status,
            sess.created_at,
            sess.last_seen_at,
            sess.expires_at
        FROM sessions sess
        INNER JOIN subjects s ON s.id = sess.subject_id
        INNER JOIN devices d ON d.id = sess.device_id
        WHERE sess.refresh_token_hash = ?1
          AND sess.status = 'ACTIVE'
          AND sess.expires_at > ?2
        LIMIT 1
        "#,
    )
    .bind(refresh_token_hash)
    .bind(now_text)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized(
            "INVALID_REFRESH_TOKEN",
            "refresh token is invalid, expired, or revoked",
        ));
    };

    Ok(SessionContext {
        subject: map_subject_row(&row),
        session: map_session_row(&row),
    })
}

async fn load_subject_sessions(
    pool: &SqlitePool,
    subject_id: &str,
    current_session_id: &str,
) -> Result<Vec<SessionResponse>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT
            sess.id AS session_id,
            sess.device_id,
            d.label AS device_label,
            d.user_agent,
            sess.login_method,
            sess.status AS session_status,
            sess.created_at,
            sess.last_seen_at,
            sess.expires_at
        FROM sessions sess
        INNER JOIN devices d ON d.id = sess.device_id
        WHERE sess.subject_id = ?1
        ORDER BY sess.created_at DESC
        "#,
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from)?;

    Ok(rows
        .iter()
        .map(|row| session_record_to_response(&map_session_row(row), current_session_id))
        .collect())
}

fn map_subject_row(row: &SqliteRow) -> SubjectResponse {
    SubjectResponse {
        id: row.get("id"),
        subject_type: row
            .get::<String, _>("subject_type")
            .parse()
            .unwrap_or(SubjectType::Member),
        display_name: row.get("display_name"),
        status: row.get("status"),
    }
}

fn map_session_row(row: &SqliteRow) -> SessionRecord {
    SessionRecord {
        session_id: row.get("session_id"),
        device_id: row.get("device_id"),
        device_label: row.get("device_label"),
        user_agent: row.get("user_agent"),
        login_method: row.get("login_method"),
        status: row.get("session_status"),
        created_at: row.get("created_at"),
        last_seen_at: row.get("last_seen_at"),
        expires_at: row.get("expires_at"),
    }
}

fn session_record_to_response(record: &SessionRecord, current_session_id: &str) -> SessionResponse {
    SessionResponse {
        session_id: record.session_id.clone(),
        device_id: record.device_id.clone(),
        device_label: record.device_label.clone(),
        user_agent: record.user_agent.clone(),
        login_method: record.login_method.clone(),
        status: record.status.clone(),
        created_at: record.created_at.clone(),
        last_seen_at: record.last_seen_at.clone(),
        expires_at: record.expires_at.clone(),
        is_current: record.session_id == current_session_id,
    }
}

fn verify_password(password: &str, password_hash: &str) -> Result<(), ApiError> {
    let parsed_hash = PasswordHash::new(password_hash).map_err(|_| {
        ApiError::internal(
            "PASSWORD_HASH_INVALID",
            "stored password hash is invalid and cannot be verified",
        )
    })?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            ApiError::unauthorized("INVALID_CREDENTIALS", "identifier or password is incorrect")
        })
}

fn create_access_token(
    state: &AppState,
    subject: &SubjectResponse,
    session_id: &str,
    now: OffsetDateTime,
) -> Result<String, ApiError> {
    let claims = AccessTokenClaims {
        sub: subject.id.clone(),
        session_id: session_id.to_string(),
        subject_type: subject.subject_type,
        exp: (now + Duration::minutes(ACCESS_TOKEN_TTL_MINUTES)).unix_timestamp(),
        iat: now.unix_timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| {
        ApiError::internal(
            "ACCESS_TOKEN_ISSUE_FAILED",
            "failed to issue access token for authenticated subject",
        )
    })
}

fn decode_access_token(state: &AppState, token: &str) -> Result<AccessTokenClaims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| {
        ApiError::unauthorized("INVALID_ACCESS_TOKEN", "access token is missing or invalid")
    })
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|header_value| header_value.to_str().ok())
        .ok_or_else(|| {
            ApiError::unauthorized(
                "MISSING_AUTHORIZATION",
                "authorization header with Bearer token is required",
            )
        })?;

    value.strip_prefix("Bearer ").ok_or_else(|| {
        ApiError::unauthorized(
            "INVALID_AUTHORIZATION",
            "authorization header must use Bearer token",
        )
    })
}

pub(crate) fn normalize_identifier(identifier: &str) -> String {
    let value = identifier.trim();
    if value.contains('@') {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

fn device_label_from_user_agent(user_agent: &str) -> String {
    if user_agent.trim().is_empty() {
        return "Unknown device".to_string();
    }

    user_agent.chars().take(80).collect()
}

fn generate_refresh_token() -> String {
    let random = Uuid::new_v4().as_bytes().to_vec();
    let suffix = URL_SAFE_NO_PAD.encode(random);
    format!("rt_{}_{}", Uuid::new_v4().simple(), suffix)
}

fn generate_otp_code(challenge_id: &str, identifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(challenge_id.as_bytes());
    hasher.update(identifier.as_bytes());
    hasher.update(Uuid::new_v4().as_bytes());
    let digest = hasher.finalize();

    let mut number_bytes = [0_u8; 8];
    number_bytes.copy_from_slice(&digest[..8]);
    let number = u64::from_be_bytes(number_bytes) % 1_000_000;

    format!("{number:06}")
}

fn hash_refresh_token(token: impl AsRef<str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_ref().as_bytes());
    let digest = hasher.finalize();

    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mask_delivery_target(value: &str) -> String {
    if value.contains('@') {
        let mut parts = value.split('@');
        let name = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        let prefix: String = name.chars().take(2).collect();
        return format!("{prefix}***@{domain}");
    }

    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("***{suffix}")
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, ApiError> {
    value.format(&Rfc3339).map_err(|_| {
        ApiError::internal(
            "TIMESTAMP_FORMAT_FAILED",
            "failed to format server timestamp for auth record",
        )
    })
}

pub async fn not_found(_request: Request) -> Response {
    ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "requested resource was not found")
        .into_response()
}
