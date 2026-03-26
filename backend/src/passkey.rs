use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    AppState, SharedState,
    audit::write_audit_log,
    auth::{self, AuthResponse, SubjectResponse, SubjectType},
    error::ApiError,
    rate_limit::{
        details_json as rate_limit_details_json, enforce_rate_limit, identifier_key, subject_key,
    },
};

const PASSKEY_CHALLENGE_TTL_MINUTES: i64 = 5;
const PASSKEY_REGISTER_OPTIONS_RATE_LIMIT: u32 = 5;
const PASSKEY_LOGIN_OPTIONS_RATE_LIMIT: u32 = 5;
const PASSKEY_RATE_LIMIT_WINDOW_MINUTES: i64 = 10;

#[derive(Debug, Clone)]
pub enum PasskeyChallengeKind {
    Registration,
    Authentication,
}

#[derive(Debug, Clone)]
pub struct PasskeyChallengeEntry {
    pub kind: PasskeyChallengeKind,
    pub subject: SubjectResponse,
    pub challenge: String,
    pub expected_origin: String,
    pub rp_id: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct PasskeyRegisterOptionsRequest {
    #[serde(default)]
    pub authenticator_attachment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PasskeyRegisterVerifyRequest {
    pub challenge_id: String,
    pub credential: RegistrationCredentialPayload,
}

#[derive(Debug, Deserialize)]
pub struct PasskeyLoginOptionsRequest {
    pub subject_type: String,
    pub identifier: String,
}

#[derive(Debug, Deserialize)]
pub struct PasskeyLoginVerifyRequest {
    pub challenge_id: String,
    pub credential: AuthenticationCredentialPayload,
}

#[derive(Debug, Serialize)]
pub struct PasskeyRegisterOptionsResponse {
    pub challenge_id: String,
    pub expires_at: String,
    pub public_key: CredentialCreationOptionsJson,
}

#[derive(Debug, Serialize)]
pub struct PasskeyRegisterVerifyResponse {
    pub success: bool,
    pub message: String,
    pub credential_id: String,
    pub authenticator_label: String,
}

#[derive(Debug, Serialize)]
pub struct PasskeyLoginOptionsResponse {
    pub challenge_id: String,
    pub expires_at: String,
    pub public_key: CredentialRequestOptionsJson,
    pub credential_count: usize,
}

#[derive(Debug, Serialize)]
pub struct CredentialCreationOptionsJson {
    pub rp: RelyingPartyJson,
    pub user: PublicKeyUserJson,
    pub challenge: String,
    pub timeout: u32,
    pub attestation: String,
    pub exclude_credentials: Vec<CredentialDescriptorJson>,
    pub authenticator_selection: AuthenticatorSelectionJson,
    pub pub_key_cred_params: Vec<PublicKeyCredentialParameterJson>,
}

#[derive(Debug, Serialize)]
pub struct CredentialRequestOptionsJson {
    pub challenge: String,
    pub timeout: u32,
    pub rp_id: String,
    pub allow_credentials: Vec<CredentialDescriptorJson>,
    pub user_verification: String,
}

#[derive(Debug, Serialize)]
pub struct RelyingPartyJson {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct PublicKeyUserJson {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct CredentialDescriptorJson {
    pub r#type: String,
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transports: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthenticatorSelectionJson {
    pub resident_key: String,
    pub user_verification: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticator_attachment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PublicKeyCredentialParameterJson {
    pub r#type: String,
    pub alg: i32,
}

#[derive(Debug, Deserialize)]
pub struct RegistrationCredentialPayload {
    pub id: String,
    pub raw_id: String,
    pub r#type: String,
    #[serde(default)]
    pub authenticator_attachment: Option<String>,
    pub response: RegistrationCredentialResponsePayload,
}

#[derive(Debug, Deserialize)]
pub struct RegistrationCredentialResponsePayload {
    pub client_data_json: String,
    pub attestation_object: String,
    #[serde(default)]
    pub transports: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthenticationCredentialPayload {
    pub id: String,
    pub raw_id: String,
    pub r#type: String,
    pub response: AuthenticationCredentialResponsePayload,
}

#[derive(Debug, Deserialize)]
pub struct AuthenticationCredentialResponsePayload {
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    #[serde(default, rename = "user_handle")]
    pub _user_handle: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClientDataPayload {
    #[serde(rename = "type")]
    pub data_type: String,
    pub challenge: String,
    pub origin: String,
}

#[derive(Debug)]
struct PasskeyCredentialRecord {
    credential_id: String,
    transports: Vec<String>,
}

#[derive(Debug)]
struct PasskeyLoginRecord {
    subject: SubjectResponse,
}

pub async fn register_options(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<PasskeyRegisterOptionsRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<PasskeyRegisterOptionsResponse>, ApiError> {
    let auth_session = auth::authenticate_bearer(&state, &headers).await?;
    let rate_limit_status = match enforce_rate_limit(
        &state.rate_limit_cache,
        subject_key("passkey-register-options", &auth_session.subject.id),
        PASSKEY_REGISTER_OPTIONS_RATE_LIMIT,
        Duration::minutes(PASSKEY_RATE_LIMIT_WINDOW_MINUTES),
        "PASSKEY_REGISTER_OPTIONS_RATE_LIMITED",
        "passkey register options rate limit exceeded",
    )
    .await
    {
        Ok(status) => status,
        Err(error) => {
            let _ = write_audit_log(
                &state.db,
                Some(&auth_session.subject),
                Some(auth_session.subject.subject_type),
                None,
                "PASSKEY_REGISTER_OPTIONS_RATE_LIMITED",
                json!({
                    "limit": PASSKEY_REGISTER_OPTIONS_RATE_LIMIT,
                    "window_minutes": PASSKEY_RATE_LIMIT_WINDOW_MINUTES
                }),
            )
            .await;
            return Err(error);
        }
    };
    let request = payload.map(|Json(value)| value).unwrap_or(PasskeyRegisterOptionsRequest {
        authenticator_attachment: None,
    });
    let (expected_origin, rp_id) = resolve_origin_and_rp_id(&headers);
    let challenge = generate_passkey_challenge();
    let challenge_id = Uuid::new_v4().to_string();
    let expires_at =
        format_timestamp(OffsetDateTime::now_utc() + Duration::minutes(PASSKEY_CHALLENGE_TTL_MINUTES))?;
    let existing_credentials = load_subject_passkey_credentials(&state.db, &auth_session.subject.id).await?;

    state
        .passkey_cache
        .insert(
            challenge_id.clone(),
            PasskeyChallengeEntry {
                kind: PasskeyChallengeKind::Registration,
                subject: auth_session.subject.clone(),
                challenge: challenge.clone(),
                expected_origin: expected_origin.clone(),
                rp_id: rp_id.clone(),
                expires_at: expires_at.clone(),
            },
        )
        .await;

    let _ = write_audit_log(
        &state.db,
        Some(&auth_session.subject),
        Some(auth_session.subject.subject_type),
        None,
        "PASSKEY_REGISTER_OPTIONS_ISSUED",
        json!({
            "challenge_id": challenge_id,
            "rp_id": rp_id,
            "origin": expected_origin,
            "rate_limit": rate_limit_details_json(
                PASSKEY_REGISTER_OPTIONS_RATE_LIMIT,
                &rate_limit_status
            )
        }),
    )
    .await;

    Ok(Json(PasskeyRegisterOptionsResponse {
        challenge_id,
        expires_at,
        public_key: CredentialCreationOptionsJson {
            rp: RelyingPartyJson {
                id: rp_id,
                name: "Multi-Subject Auth System".to_string(),
            },
            user: PublicKeyUserJson {
                id: auth_session.subject.id.clone(),
                name: auth_session.subject.id.clone(),
                display_name: auth_session.subject.display_name.clone(),
            },
            challenge,
            timeout: 60_000,
            attestation: "none".to_string(),
            exclude_credentials: existing_credentials
                .into_iter()
                .map(|record| CredentialDescriptorJson {
                    r#type: "public-key".to_string(),
                    id: record.credential_id,
                    transports: record.transports,
                })
                .collect(),
            authenticator_selection: AuthenticatorSelectionJson {
                resident_key: "preferred".to_string(),
                user_verification: "preferred".to_string(),
                authenticator_attachment: request.authenticator_attachment,
            },
            pub_key_cred_params: vec![
                PublicKeyCredentialParameterJson {
                    r#type: "public-key".to_string(),
                    alg: -7,
                },
                PublicKeyCredentialParameterJson {
                    r#type: "public-key".to_string(),
                    alg: -257,
                },
            ],
        },
    }))
}

pub async fn register_verify(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<PasskeyRegisterVerifyRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<PasskeyRegisterVerifyResponse>, ApiError> {
    let auth_session = auth::authenticate_bearer(&state, &headers).await?;
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with challenge_id and credential",
        )
    })?;
    let challenge_id = request.challenge_id.trim().to_string();
    if challenge_id.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "challenge_id is required",
        ));
    }

    let challenge = load_valid_challenge(&state, &challenge_id, PasskeyChallengeKind::Registration).await?;
    if challenge.subject.id != auth_session.subject.id {
        return Err(ApiError::forbidden(
            "PASSKEY_SUBJECT_MISMATCH",
            "current subject does not match passkey registration challenge",
        ));
    }

    validate_registration_credential(&request.credential, &challenge)?;

    let credential_id = normalize_credential_id(&request.credential);
    if credential_id.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_CREDENTIAL",
            "credential id is required",
        ));
    }

    let authenticator_label = build_authenticator_label(
        request.credential.authenticator_attachment.as_deref(),
        headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()),
    );
    let now_text = format_timestamp(OffsetDateTime::now_utc())?;
    let transports_json = serde_json::to_string(&request.credential.response.transports)
        .map_err(|_| ApiError::internal("PASSKEY_SERIALIZATION_FAILED", "failed to serialize passkey transports"))?;

    let exists = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(1)
        FROM passkey_credentials
        WHERE credential_id = ?1
        "#,
    )
    .bind(&credential_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    if exists > 0 {
        let _ = write_audit_log(
            &state.db,
            Some(&auth_session.subject),
            Some(auth_session.subject.subject_type),
            None,
            "PASSKEY_REGISTER_FAILED",
            json!({"reason":"CREDENTIAL_ALREADY_EXISTS","challenge_id":challenge_id}),
        )
        .await;
        return Err(ApiError::bad_request(
            "PASSKEY_ALREADY_REGISTERED",
            "passkey credential is already registered",
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO passkey_credentials (
            id,
            subject_id,
            credential_id,
            credential_public_key,
            attestation_object,
            client_data_json,
            transports_json,
            authenticator_attachment,
            authenticator_label,
            sign_count,
            is_enabled,
            created_at,
            last_used_at
        )
        VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, 0, 1, ?9, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&auth_session.subject.id)
    .bind(&credential_id)
    .bind(&request.credential.response.attestation_object)
    .bind(&request.credential.response.client_data_json)
    .bind(transports_json)
    .bind(request.credential.authenticator_attachment.as_deref())
    .bind(&authenticator_label)
    .bind(&now_text)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    state.passkey_cache.invalidate(&challenge_id).await;

    let _ = write_audit_log(
        &state.db,
        Some(&auth_session.subject),
        Some(auth_session.subject.subject_type),
        None,
        "PASSKEY_REGISTERED",
        json!({
            "challenge_id": challenge_id,
            "credential_id": credential_id,
            "authenticator_label": authenticator_label
        }),
    )
    .await;

    Ok(Json(PasskeyRegisterVerifyResponse {
        success: true,
        message: "passkey registered for current subject".to_string(),
        credential_id,
        authenticator_label,
    }))
}

pub async fn login_options(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<PasskeyLoginOptionsRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<PasskeyLoginOptionsResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with subject_type and identifier",
        )
    })?;

    let subject_type: SubjectType = request.subject_type.parse()?;
    let identifier = auth::normalize_identifier(&request.identifier);
    if identifier.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "identifier is required",
        ));
    }

    let rate_limit_status = match enforce_rate_limit(
        &state.rate_limit_cache,
        identifier_key("passkey-login-options", subject_type.as_str(), &identifier),
        PASSKEY_LOGIN_OPTIONS_RATE_LIMIT,
        Duration::minutes(PASSKEY_RATE_LIMIT_WINDOW_MINUTES),
        "PASSKEY_LOGIN_OPTIONS_RATE_LIMITED",
        "passkey login options rate limit exceeded",
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
                "PASSKEY_LOGIN_OPTIONS_RATE_LIMITED",
                json!({
                    "limit": PASSKEY_LOGIN_OPTIONS_RATE_LIMIT,
                    "window_minutes": PASSKEY_RATE_LIMIT_WINDOW_MINUTES
                }),
            )
            .await;
            return Err(error);
        }
    };

    let subject = find_subject_by_identifier(&state.db, subject_type, &identifier).await?;
    let credentials = load_subject_passkey_credentials(&state.db, &subject.id).await?;
    if credentials.is_empty() {
        return Err(ApiError::unauthorized(
            "PASSKEY_NOT_REGISTERED",
            "no passkey credential is registered for this subject",
        ));
    }

    let (expected_origin, rp_id) = resolve_origin_and_rp_id(&headers);
    let challenge = generate_passkey_challenge();
    let challenge_id = Uuid::new_v4().to_string();
    let expires_at =
        format_timestamp(OffsetDateTime::now_utc() + Duration::minutes(PASSKEY_CHALLENGE_TTL_MINUTES))?;

    state
        .passkey_cache
        .insert(
            challenge_id.clone(),
            PasskeyChallengeEntry {
                kind: PasskeyChallengeKind::Authentication,
                subject: subject.clone(),
                challenge: challenge.clone(),
                expected_origin: expected_origin.clone(),
                rp_id: rp_id.clone(),
                expires_at: expires_at.clone(),
            },
        )
        .await;

    let _ = write_audit_log(
        &state.db,
        Some(&subject),
        Some(subject.subject_type),
        Some(&identifier),
        "PASSKEY_LOGIN_OPTIONS_ISSUED",
        json!({
            "challenge_id": challenge_id,
            "credential_count": credentials.len(),
            "rp_id": rp_id,
            "origin": expected_origin,
            "rate_limit": rate_limit_details_json(
                PASSKEY_LOGIN_OPTIONS_RATE_LIMIT,
                &rate_limit_status
            )
        }),
    )
    .await;

    Ok(Json(PasskeyLoginOptionsResponse {
        challenge_id,
        expires_at,
        credential_count: credentials.len(),
        public_key: CredentialRequestOptionsJson {
            challenge,
            timeout: 60_000,
            rp_id,
            allow_credentials: credentials
                .into_iter()
                .map(|record| CredentialDescriptorJson {
                    r#type: "public-key".to_string(),
                    id: record.credential_id,
                    transports: record.transports,
                })
                .collect(),
            user_verification: "preferred".to_string(),
        },
    }))
}

pub async fn login_verify(
    State(state): State<SharedState>,
    headers: HeaderMap,
    payload: Result<Json<PasskeyLoginVerifyRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<AuthResponse>, ApiError> {
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "INVALID_REQUEST",
            "request body must be valid JSON with challenge_id and credential",
        )
    })?;
    let challenge_id = request.challenge_id.trim().to_string();
    if challenge_id.is_empty() {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST",
            "challenge_id is required",
        ));
    }

    let challenge = load_valid_challenge(&state, &challenge_id, PasskeyChallengeKind::Authentication).await?;
    validate_authentication_credential(&request.credential, &challenge)?;

    let credential_id = normalize_credential_id(&request.credential);
    let login_record = find_passkey_login_record(&state.db, &credential_id).await?;
    if login_record.subject.id != challenge.subject.id {
        return Err(ApiError::unauthorized(
            "PASSKEY_SUBJECT_MISMATCH",
            "passkey credential does not belong to requested subject",
        ));
    }

    let now_text = format_timestamp(OffsetDateTime::now_utc())?;
    sqlx::query(
        r#"
        UPDATE passkey_credentials
        SET last_used_at = ?1
        WHERE credential_id = ?2
        "#,
    )
    .bind(&now_text)
    .bind(&credential_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    state.passkey_cache.invalidate(&challenge_id).await;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("Unknown device");
    let response =
        auth::create_session_and_tokens(&state, login_record.subject.clone(), user_agent, "PASSKEY")
            .await?;

    let _ = write_audit_log(
        &state.db,
        Some(&login_record.subject),
        Some(login_record.subject.subject_type),
        None,
        "PASSKEY_LOGIN_SUCCESS",
        json!({
            "challenge_id": challenge_id,
            "credential_id": credential_id,
            "session_id": response.session.session_id
        }),
    )
    .await;

    Ok(Json(response))
}

async fn find_subject_by_identifier(
    pool: &SqlitePool,
    subject_type: SubjectType,
    identifier: &str,
) -> Result<SubjectResponse, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name
        FROM subjects s
        INNER JOIN subject_identifiers si ON si.subject_id = s.id
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
            "SUBJECT_NOT_FOUND",
            "subject was not found for passkey login",
        ));
    };

    Ok(SubjectResponse {
        id: row.get("id"),
        subject_type: row
            .get::<String, _>("subject_type")
            .parse()
            .unwrap_or(SubjectType::Member),
        display_name: row.get("display_name"),
        status: row.get("status"),
    })
}

async fn load_subject_passkey_credentials(
    pool: &SqlitePool,
    subject_id: &str,
) -> Result<Vec<PasskeyCredentialRecord>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT credential_id, transports_json
        FROM passkey_credentials
        WHERE subject_id = ?1
          AND is_enabled = 1
        ORDER BY created_at DESC
        "#,
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from)?;

    rows.into_iter()
        .map(|row| {
            let transports = row
                .try_get::<String, _>("transports_json")
                .ok()
                .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
                .unwrap_or_default();
            Ok(PasskeyCredentialRecord {
                credential_id: row.get("credential_id"),
                transports,
            })
        })
        .collect()
}

async fn find_passkey_login_record(
    pool: &SqlitePool,
    credential_id: &str,
) -> Result<PasskeyLoginRecord, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id,
            s.subject_type,
            s.status,
            s.display_name
        FROM passkey_credentials pc
        INNER JOIN subjects s ON s.id = pc.subject_id
        WHERE pc.credential_id = ?1
          AND pc.is_enabled = 1
          AND s.status = 'ACTIVE'
        LIMIT 1
        "#,
    )
    .bind(credential_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized(
            "PASSKEY_CREDENTIAL_NOT_FOUND",
            "passkey credential was not found or is disabled",
        ));
    };

    Ok(PasskeyLoginRecord {
        subject: SubjectResponse {
            id: row.get("id"),
            subject_type: row
                .get::<String, _>("subject_type")
                .parse()
                .unwrap_or(SubjectType::Member),
            display_name: row.get("display_name"),
            status: row.get("status"),
        },
    })
}

async fn load_valid_challenge(
    state: &AppState,
    challenge_id: &str,
    expected_kind: PasskeyChallengeKind,
) -> Result<PasskeyChallengeEntry, ApiError> {
    let Some(entry) = state.passkey_cache.get(challenge_id).await else {
        return Err(ApiError::unauthorized(
            "PASSKEY_CHALLENGE_NOT_FOUND",
            "passkey challenge is invalid, expired, or already used",
        ));
    };

    let kind_matches = matches!(
        (&entry.kind, &expected_kind),
        (PasskeyChallengeKind::Registration, PasskeyChallengeKind::Registration)
            | (PasskeyChallengeKind::Authentication, PasskeyChallengeKind::Authentication)
    );
    if !kind_matches {
        return Err(ApiError::bad_request(
            "PASSKEY_CHALLENGE_KIND_MISMATCH",
            "passkey challenge kind does not match requested operation",
        ));
    }

    let now_text = format_timestamp(OffsetDateTime::now_utc())?;
    if entry.expires_at <= now_text {
        state.passkey_cache.invalidate(challenge_id).await;
        return Err(ApiError::unauthorized(
            "PASSKEY_CHALLENGE_EXPIRED",
            "passkey challenge is expired",
        ));
    }

    Ok(entry)
}

fn validate_registration_credential(
    credential: &RegistrationCredentialPayload,
    challenge: &PasskeyChallengeEntry,
) -> Result<(), ApiError> {
    if credential.r#type != "public-key" {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_CREDENTIAL",
            "credential type must be public-key",
        ));
    }

    let client_data = parse_client_data(&credential.response.client_data_json)?;
    if client_data.data_type != "webauthn.create" {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_CLIENT_DATA",
            "client data type must be webauthn.create",
        ));
    }

    validate_client_data(&client_data, challenge)
}

fn validate_authentication_credential(
    credential: &AuthenticationCredentialPayload,
    challenge: &PasskeyChallengeEntry,
) -> Result<(), ApiError> {
    if credential.r#type != "public-key" {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_CREDENTIAL",
            "credential type must be public-key",
        ));
    }

    if credential.response.signature.trim().is_empty()
        || credential.response.authenticator_data.trim().is_empty()
    {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_ASSERTION",
            "authenticator_data and signature are required",
        ));
    }

    let client_data = parse_client_data(&credential.response.client_data_json)?;
    if client_data.data_type != "webauthn.get" {
        return Err(ApiError::bad_request(
            "INVALID_PASSKEY_CLIENT_DATA",
            "client data type must be webauthn.get",
        ));
    }

    validate_client_data(&client_data, challenge)
}

fn validate_client_data(
    client_data: &ClientDataPayload,
    challenge: &PasskeyChallengeEntry,
) -> Result<(), ApiError> {
    if client_data.challenge != challenge.challenge {
        return Err(ApiError::unauthorized(
            "PASSKEY_CHALLENGE_MISMATCH",
            "passkey challenge does not match client data",
        ));
    }

    if client_data.origin != challenge.expected_origin {
        return Err(ApiError::unauthorized(
            "PASSKEY_ORIGIN_MISMATCH",
            "passkey origin does not match issued challenge",
        ));
    }

    let origin_host = client_data
        .origin
        .split("://")
        .nth(1)
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if origin_host != challenge.rp_id {
        return Err(ApiError::unauthorized(
            "PASSKEY_RP_ID_MISMATCH",
            "passkey rp_id does not match issued challenge",
        ));
    }

    Ok(())
}

fn parse_client_data(value: &str) -> Result<ClientDataPayload, ApiError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::bad_request("INVALID_PASSKEY_CLIENT_DATA", "client_data_json must be base64url encoded"))?;
    serde_json::from_slice::<ClientDataPayload>(&bytes).map_err(|_| {
        ApiError::bad_request(
            "INVALID_PASSKEY_CLIENT_DATA",
            "client_data_json must decode to valid client data JSON",
        )
    })
}

fn normalize_credential_id<T>(credential: &T) -> String
where
    T: CredentialIdSource,
{
    let raw_id = credential.raw_id().trim();
    if raw_id.is_empty() {
        credential.id().trim().to_string()
    } else {
        raw_id.to_string()
    }
}

fn resolve_origin_and_rp_id(headers: &HeaderMap) -> (String, String) {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("http://127.0.0.1:3000")
        .to_string();
    let host = origin
        .split("://")
        .nth(1)
        .unwrap_or("127.0.0.1:3000")
        .split('/')
        .next()
        .unwrap_or("127.0.0.1:3000");
    let rp_id = host.split(':').next().unwrap_or("127.0.0.1").to_string();

    (origin, rp_id)
}

fn build_authenticator_label(
    authenticator_attachment: Option<&str>,
    user_agent: Option<&str>,
) -> String {
    match authenticator_attachment {
        Some("platform") => "Platform Passkey".to_string(),
        Some("cross-platform") => "Cross-platform Passkey".to_string(),
        _ => user_agent
            .map(|value| format!("Passkey on {}", value.chars().take(32).collect::<String>()))
            .unwrap_or_else(|| "Passkey Credential".to_string()),
    }
}

fn generate_passkey_challenge() -> String {
    let mut hasher = Sha256::new();
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(OffsetDateTime::now_utc().unix_timestamp_nanos().to_be_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(digest)
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, ApiError> {
    value.format(&Rfc3339).map_err(|_| {
        ApiError::internal(
            "TIMESTAMP_FORMAT_FAILED",
            "failed to format server timestamp for passkey record",
        )
    })
}

trait CredentialIdSource {
    fn id(&self) -> &str;
    fn raw_id(&self) -> &str;
}

impl CredentialIdSource for RegistrationCredentialPayload {
    fn id(&self) -> &str {
        &self.id
    }

    fn raw_id(&self) -> &str {
        &self.raw_id
    }
}

impl CredentialIdSource for AuthenticationCredentialPayload {
    fn id(&self) -> &str {
        &self.id
    }

    fn raw_id(&self) -> &str {
        &self.raw_id
    }
}
