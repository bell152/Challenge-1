use axum::{
    Json,
    extract::State,
    http::HeaderMap,
};
use serde::Serialize;
use serde_json::json;

use crate::{
    AppState, SharedState, access,
    audit::write_audit_log,
    auth::{self, SubjectType},
    error::ApiError,
};

#[derive(Debug, Serialize)]
pub struct PortalHomeResponse {
    pub portal_key: String,
    pub portal_title: String,
    pub allowed_subject_type: SubjectType,
    pub summary: String,
    pub highlights: Vec<PortalHighlight>,
}

#[derive(Debug, Serialize)]
pub struct PortalHighlight {
    pub label: String,
    pub value: String,
    pub note: String,
}

pub async fn member_home(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<PortalHomeResponse>, ApiError> {
    portal_home(&state, &headers, SubjectType::Member).await
}

pub async fn community_home(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<PortalHomeResponse>, ApiError> {
    portal_home(&state, &headers, SubjectType::CommunityStaff).await
}

pub async fn platform_home(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<PortalHomeResponse>, ApiError> {
    portal_home(&state, &headers, SubjectType::PlatformStaff).await
}

async fn portal_home(
    state: &AppState,
    headers: &HeaderMap,
    required_subject_type: SubjectType,
) -> Result<Json<PortalHomeResponse>, ApiError> {
    let authenticated = auth::authenticate_bearer(state, headers).await?;

    if let Err(error) = access::require_subject_type(&authenticated.subject, required_subject_type) {
        let _ = write_audit_log(
            &state.db,
            Some(&authenticated.subject),
            Some(authenticated.subject.subject_type),
            None,
            "PORTAL_ACCESS_DENIED",
            json!({
                "portal": portal_key(required_subject_type),
                "required_subject_type": required_subject_type.as_str(),
                "actual_subject_type": authenticated.subject.subject_type.as_str(),
                "session_id": authenticated.session.session_id
            }),
        )
        .await;
        return Err(error);
    }

    let response = portal_payload(&authenticated.subject.display_name, required_subject_type);
    let _ = write_audit_log(
        &state.db,
        Some(&authenticated.subject),
        Some(authenticated.subject.subject_type),
        None,
        "PORTAL_ACCESS_GRANTED",
        json!({
            "portal": response.portal_key,
            "session_id": authenticated.session.session_id
        }),
    )
    .await;

    Ok(Json(response))
}

fn portal_payload(display_name: &str, subject_type: SubjectType) -> PortalHomeResponse {
    match subject_type {
        SubjectType::Member => PortalHomeResponse {
            portal_key: "member".to_string(),
            portal_title: "Member Portal".to_string(),
            allowed_subject_type: SubjectType::Member,
            summary: format!(
                "{display_name} 当前进入的是 Member 视角，用于说明会员主体与其他 staff 门户的边界分离。"
            ),
            highlights: vec![
                PortalHighlight {
                    label: "Primary Goal".to_string(),
                    value: "Self-service account access".to_string(),
                    note: "强调会员只看到自己的入口与会话能力。".to_string(),
                },
                PortalHighlight {
                    label: "Supported OTP Channels".to_string(),
                    value: "Email / Phone".to_string(),
                    note: "Member 同时支持 email 与 phone 的 OTP 通道。".to_string(),
                },
                PortalHighlight {
                    label: "Architecture Focus".to_string(),
                    value: "Password + OTP + multi-session".to_string(),
                    note: "用于说明统一 subject 模型下的多认证方式。".to_string(),
                },
            ],
        },
        SubjectType::CommunityStaff => PortalHomeResponse {
            portal_key: "community".to_string(),
            portal_title: "Community Staff Portal".to_string(),
            allowed_subject_type: SubjectType::CommunityStaff,
            summary: format!(
                "{display_name} 当前进入的是 Community Staff 视角，用于体现 staff 门户和 member 门户的边界不同。"
            ),
            highlights: vec![
                PortalHighlight {
                    label: "Primary Goal".to_string(),
                    value: "Community operations".to_string(),
                    note: "这里只展示社区侧 staff 的门户卡片，不混入平台侧入口。".to_string(),
                },
                PortalHighlight {
                    label: "Supported OTP Channels".to_string(),
                    value: "Email".to_string(),
                    note: "当前 seed 只给 staff 配置 email OTP 通道。".to_string(),
                },
                PortalHighlight {
                    label: "Architecture Focus".to_string(),
                    value: "Boundary over complexity".to_string(),
                    note: "不做复杂 RBAC，但先把主体边界守住。".to_string(),
                },
            ],
        },
        SubjectType::PlatformStaff => PortalHomeResponse {
            portal_key: "platform".to_string(),
            portal_title: "Platform Staff Portal".to_string(),
            allowed_subject_type: SubjectType::PlatformStaff,
            summary: format!(
                "{display_name} 当前进入的是 Platform Staff 视角，用于体现平台运营主体与社区 staff 的边界感。"
            ),
            highlights: vec![
                PortalHighlight {
                    label: "Primary Goal".to_string(),
                    value: "Platform operations".to_string(),
                    note: "与 Community Staff 门户分开，便于说明主体差异。".to_string(),
                },
                PortalHighlight {
                    label: "Supported OTP Channels".to_string(),
                    value: "Email".to_string(),
                    note: "平台 staff 当前也通过 email OTP 展示第二条登录路径。".to_string(),
                },
                PortalHighlight {
                    label: "Architecture Focus".to_string(),
                    value: "Authentication vs authorization".to_string(),
                    note: "先完成身份确认，再由 portal 边界控制访问入口。".to_string(),
                },
            ],
        },
    }
}

fn portal_key(subject_type: SubjectType) -> &'static str {
    match subject_type {
        SubjectType::Member => "member",
        SubjectType::CommunityStaff => "community",
        SubjectType::PlatformStaff => "platform",
    }
}
