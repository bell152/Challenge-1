use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{
    SharedState,
    auth::{
        list_sessions, logout, logout_all, me, not_found, otp_request, otp_verify,
        password_login, refresh, revoke_session,
    },
    health,
    passkey::{login_options, login_verify, register_options, register_verify},
    portal::{community_home, member_home, platform_home},
};

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/password/login", post(password_login))
        .route("/api/auth/otp/request", post(otp_request))
        .route("/api/auth/otp/verify", post(otp_verify))
        .route("/api/auth/passkey/register/options", post(register_options))
        .route("/api/auth/passkey/register/verify", post(register_verify))
        .route("/api/auth/passkey/login/options", post(login_options))
        .route("/api/auth/passkey/login/verify", post(login_verify))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/logout-all", post(logout_all))
        .route("/api/auth/me", get(me))
        .route("/api/auth/sessions", get(list_sessions))
        .route("/api/auth/sessions/{id}", delete(revoke_session))
        .route("/api/portal/member/home", get(member_home))
        .route("/api/portal/community/home", get(community_home))
        .route("/api/portal/platform/home", get(platform_home))
        .fallback(not_found)
        .with_state(state)
}
