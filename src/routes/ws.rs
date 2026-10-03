use actix_web::{web, HttpRequest, HttpResponse};
use redis::AsyncCommands;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::tokens::verify_access_token;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{redis_pubsub, user_service};
use crate::ws::record_legacy_token_auth;
use crate::ws::session::run_ws_session;

// ── Request DTO ───────────────────────────────────────────────────────────────

/// No `Debug` derive: `token` is a full access JWT and must never be formatted
/// into a log line by accident.
#[derive(Deserialize)]
pub struct WsQuery {
    ticket: Option<String>,
    /// Deprecated handshake from installed apps (<= 1.3.2); see `ws_allow_legacy_token`.
    token: Option<String>,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// GET /ws?ticket=<single-use-ticket>
///
/// Upgrades the connection after atomically consuming a short-lived ticket
/// issued by the authenticated `/auth/ws-ticket` endpoint.
///
/// Transition window: while `WS_ALLOW_LEGACY_TOKEN` is on (default), the old
/// `GET /ws?token=<access JWT>` form is still accepted so installed apps keep
/// their realtime socket. Each such handshake logs `ws_legacy_token_auth`
/// (never the token) and bumps a counter shown in the admin overview.
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    query: web::Query<WsQuery>,
    config: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
) -> Result<HttpResponse, AppError> {
    // A ticket always wins: a bad ticket is rejected rather than retried as a
    // token, so the preferred path can never be downgraded.
    let (user_id, via_legacy_token) = match (query.ticket.as_deref(), query.token.as_deref()) {
        (Some(ticket), _) => (consume_ticket(&redis, ticket).await?, false),
        (None, Some(token)) if config.ws_allow_legacy_token => {
            (user_id_from_legacy_token(token, &config.jwt_secret)?, true)
        }
        (None, Some(_)) => {
            tracing::warn!(
                user_agent = user_agent(&req),
                "ws_legacy_token_rejected: legacy ?token= handshake refused (WS_ALLOW_LEGACY_TOKEN is off)"
            );
            return Err(AppError::Unauthorized);
        }
        (None, None) => return Err(AppError::Unauthorized),
    };

    // Ensure the user exists and is active
    user_service::find_by_id(&pool, user_id)?.ok_or(AppError::Unauthorized)?;

    if via_legacy_token {
        record_legacy_token_auth();
        tracing::warn!(
            user_id = %user_id,
            user_agent = user_agent(&req),
            "ws_legacy_token_auth: accepted deprecated ?token= handshake; client should use ?ticket="
        );
    }

    // Best-effort last_seen update (non-fatal if it fails)
    let _ = user_service::update_last_seen(&pool, user_id);

    // Subscribe before upgrading: pubsub keeps nothing for absent subscribers,
    // so anything published between the 101 and a later SUBSCRIBE (a message,
    // an incoming call) would be lost while the client believes it is live.
    let channel = redis_pubsub::user_channel(user_id);
    let pubsub = redis_pubsub::subscribe(redis.get_ref(), &[channel.as_str()])
        .await
        .map_err(AppError::Internal)?;

    // Perform the HTTP → WebSocket upgrade
    let (response, session, msg_stream) =
        actix_ws::handle(&req, stream).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    let redis_client = redis.get_ref().clone();
    let db_pool = pool.get_ref().clone();
    // actix_web::rt::spawn does not require Send, which is needed because
    // MessageStream wraps a non-Send payload stream.
    actix_web::rt::spawn(run_ws_session(
        user_id,
        session,
        msg_stream,
        pubsub,
        redis_client,
        db_pool,
        config.get_ref().clone(),
    ));

    Ok(response)
}

fn user_agent(req: &HttpRequest) -> &str {
    req.headers()
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// Atomically redeem a single-use ticket for the user it was issued to.
async fn consume_ticket(redis: &redis::Client, ticket: &str) -> Result<Uuid, AppError> {
    if ticket.len() != 64 || !ticket.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Unauthorized);
    }
    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    // GETDEL prevents replay even when the subsequent WebSocket upgrade fails.
    let user_id: Option<String> = conn
        .get_del(format!("ws_ticket:{ticket}"))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis getdel: {e}")))?;
    user_id
        .and_then(|value| Uuid::parse_str(&value).ok())
        .ok_or(AppError::Unauthorized)
}

/// Legacy handshake: validate the access JWT exactly as v1.4.34 did.
fn user_id_from_legacy_token(token: &str, jwt_secret: &str) -> Result<Uuid, AppError> {
    let claims = verify_access_token(token, jwt_secret).map_err(|_| AppError::Unauthorized)?;
    Ok(claims.user_id()?)
}

/// Register the WebSocket route.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(ws_handler));
}

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test, App};
    use diesel::r2d2::{self, ConnectionManager};
    use tracing_actix_web::TracingLogger;

    use super::*;
    use crate::auth::claims::Claims;
    use crate::auth::tokens::issue_access_token;
    use crate::request_logging::test_support::LogBuffer;
    use crate::request_logging::PathOnlyRootSpanBuilder;

    const JWT_SECRET: &str = "unit-test-jwt-secret-0123456789abcdef";

    fn test_config(ws_allow_legacy_token: bool) -> Config {
        Config {
            ws_allow_legacy_token,
            ..Config::for_tests()
        }
    }

    fn jwt_signed_with(secret: &str, exp_offset: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = Claims::new(Uuid::new_v4(), "user".into(), now, now + exp_offset);
        issue_access_token(&claims, secret).unwrap()
    }

    /// Status of `GET <uri>` against the real handler. Redis and Postgres are
    /// deliberately unreachable: every case here must be decided before either
    /// is touched, which is itself part of what is being asserted.
    async fn handshake_status(allow_legacy: bool, uri: &str) -> StatusCode {
        let pool: Pool = r2d2::Pool::builder().build_unchecked(ConnectionManager::new(
            "postgres://unused@127.0.0.1:1/unused",
        ));
        let redis = redis::Client::open("redis://127.0.0.1:1").unwrap();
        let app = test::init_service(
            App::new()
                .wrap(TracingLogger::<PathOnlyRootSpanBuilder>::new())
                .app_data(web::Data::new(test_config(allow_legacy)))
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(redis))
                .service(web::scope("/ws").configure(configure)),
        )
        .await;
        test::call_service(&app, test::TestRequest::get().uri(uri).to_request())
            .await
            .status()
    }

    #[actix_web::test]
    async fn legacy_token_validates_like_v1_4_34() {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp();
        let good = issue_access_token(&Claims::new(id, "user".into(), now, now + 3600), JWT_SECRET)
            .unwrap();
        assert_eq!(user_id_from_legacy_token(&good, JWT_SECRET).unwrap(), id);

        let forged = jwt_signed_with("some-other-secret-0123456789abcdef", 3600);
        assert!(matches!(
            user_id_from_legacy_token(&forged, JWT_SECRET),
            Err(AppError::Unauthorized)
        ));
        let expired = jwt_signed_with(JWT_SECRET, -3600);
        assert!(matches!(
            user_id_from_legacy_token(&expired, JWT_SECRET),
            Err(AppError::Unauthorized)
        ));
        assert!(matches!(
            user_id_from_legacy_token("not-a-jwt", JWT_SECRET),
            Err(AppError::Unauthorized)
        ));
    }

    #[actix_web::test]
    async fn handshake_without_any_credential_is_unauthorized() {
        assert_eq!(
            handshake_status(true, "/ws").await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn malformed_ticket_is_unauthorized() {
        for ticket in ["short", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
            let uri = format!("/ws?ticket={ticket}");
            assert_eq!(
                handshake_status(true, &uri).await,
                StatusCode::UNAUTHORIZED,
                "{uri}"
            );
        }
    }

    #[actix_web::test]
    async fn bad_ticket_is_not_retried_as_a_legacy_token() {
        let valid_jwt = jwt_signed_with(JWT_SECRET, 3600);
        let uri = format!("/ws?ticket=nope&token={valid_jwt}");
        assert_eq!(handshake_status(true, &uri).await, StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn legacy_token_is_refused_when_the_flag_is_off_even_if_valid() {
        let valid_jwt = jwt_signed_with(JWT_SECRET, 3600);
        let uri = format!("/ws?token={valid_jwt}");
        assert_eq!(
            handshake_status(false, &uri).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn legacy_token_with_bad_signature_is_unauthorized_when_the_flag_is_on() {
        let forged = jwt_signed_with("some-other-secret-0123456789abcdef", 3600);
        let uri = format!("/ws?token={forged}");
        assert_eq!(handshake_status(true, &uri).await, StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn rejected_handshakes_never_write_the_credential_to_the_logs() {
        let logs = LogBuffer::default();
        let _guard = logs.capture();
        let valid_jwt = jwt_signed_with(JWT_SECRET, 3600);
        let forged = jwt_signed_with("some-other-secret-0123456789abcdef", 3600);
        let ticket = "ab".repeat(32);

        // flag off + valid JWT (emits ws_legacy_token_rejected), flag on + forged
        // JWT, and a ticket that is not in Redis-shaped form.
        handshake_status(false, &format!("/ws?token={valid_jwt}")).await;
        handshake_status(true, &format!("/ws?token={forged}")).await;
        handshake_status(true, &format!("/ws?ticket=xx{ticket}")).await;

        let out = logs.contents();
        assert!(
            out.contains("ws_legacy_token_rejected"),
            "no rejection log: {out}"
        );
        for secret in [valid_jwt.as_str(), forged.as_str(), ticket.as_str()] {
            assert!(!out.contains(secret), "credential leaked into logs: {out}");
        }
        assert!(
            !out.contains("/ws?"),
            "query string leaked into logs: {out}"
        );
        assert_eq!(
            crate::ws::legacy_token_auth_count(),
            0,
            "rejections must not count"
        );
    }
}
