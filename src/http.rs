use crate::{application::Application, contracts::*, error::Error};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
    routing::{get, post},
};
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};

#[derive(Clone)]
struct HttpState {
    app: Application,
    token: String,
    authority: String,
}
struct ApiError(Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.0 {
            Error::Invalid(message) => (StatusCode::BAD_REQUEST, "invalid_command", message),
            Error::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Error::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "Record not found".into(),
            ),
            e => {
                eprintln!("application error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Persistence or contract failure".into(),
                )
            }
        };
        (
            status,
            Json(serde_json::json!({"code":code,"message":message})),
        )
            .into_response()
    }
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> crate::error::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| ApiError(Error::Invalid("application task interrupted".into())))?
        .map_err(ApiError)
}
pub fn router(app: Application, token: String, authority: String) -> Router {
    let state = HttpState {
        app,
        token,
        authority,
    };
    let api = Router::new()
        .route("/cases", get(list).post(create))
        .route("/cases/{id}", get(snapshot))
        .route("/cases/{id}/rename", post(rename))
        .route("/cases/{id}/challenges", post(challenge))
        .route("/cases/{id}/gate", post(case_gate))
        .route("/cases/{id}/correlations", post(case_correlation))
        .route("/cases/{id}/repairs", post(request_repair))
        .route("/cases/{id}/repairs/markdown", get(repair_markdown))
        .route(
            "/cases/{id}/qualification/markdown",
            get(qualification_markdown),
        )
        .route("/cases/{id}/incident", post(declare_incident))
        .route("/cases/{id}/incident/operations", post(incident_operation))
        .route("/cases/{id}/incident/artifacts", post(incident_artifact))
        .route("/cases/{id}/incident/markdown", get(incident_markdown))
        .route("/sources/sentry", get(source_health))
        .route("/cases/{id}/team/messages", post(team_message))
        .route("/cases/{id}/team/message-ack", post(team_message_ack))
        .route("/cases/{id}/team/handoffs", post(team_handoff))
        .route("/cases/{id}/team/accept-handoff", post(team_accept))
        .route("/cases/{id}/team/escalation-ack", post(team_escalation_ack))
        .route(
            "/cases/{id}/team/escalation-resolve",
            post(escalation_resolve),
        )
        .route(
            "/cases/{id}/team/delivery-reconcile",
            post(delivery_reconcile),
        )
        .route("/team/claim", post(team_claim))
        .route("/team/heartbeat", post(team_heartbeat))
        .route("/team/progress", post(team_progress))
        .route("/team/block", post(team_block))
        .route("/team/finish", post(team_finish))
        .route("/team/escalations", get(team_inbox))
        .route("/cases/{id}/evidence", post(record_evidence))
        .route("/cases/{id}/dispositions", post(record_disposition))
        .route("/cases/{id}/reconsider", post(reconsider))
        .route("/cases/{id}/events", get(stream))
        .route("/evidence/{digest}", get(evidence))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .nest("/api/v1", api)
        .route("/integrations/sentry", post(sentry_intake))
        .route(
            "/",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    include_str!("../web/index.html"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/dist/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .route(
            "/logo.png",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/png")],
                    include_bytes!("../assets/logo.png").as_slice(),
                )
            }),
        )
        .route(
            "/icon.png",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/png")],
                    include_bytes!("../assets/icon.png").as_slice(),
                )
            }),
        )
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            local_boundary,
        ))
        .with_state(state)
}
async fn local_boundary(State(s): State<HttpState>, req: Request, next: Next) -> Response {
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(&s.authority);
    let origin_ok = req
        .headers()
        .get(header::ORIGIN)
        .is_none_or(|v| v.to_str().ok() == Some(&format!("http://{}", s.authority)));
    if !host_ok || !origin_ok {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"code":"unsafe_origin","message":"Untrusted host or origin"})),
        )
            .into_response();
    }
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; img-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res
}
async fn authenticate(State(s): State<HttpState>, req: Request, next: Next) -> Response {
    // Bearer credentials are explicit headers; there is no ambient cookie authority.
    let supplied = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if supplied != Some(s.token.as_str()) {
        return (StatusCode::UNAUTHORIZED,Json(serde_json::json!({"code":"unauthorized","message":"Enter the local operator token printed by serve"}))).into_response();
    }
    next.run(req).await
}
macro_rules! case_command {
    ($handler:ident,$command:ty,$method:ident) => {
        async fn $handler(
            State(s): State<HttpState>,
            Path(id): Path<String>,
            body: Result<Json<$command>, JsonRejection>,
        ) -> Result<impl IntoResponse, ApiError> {
            let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
            Ok(Json(blocking(move || s.app.$method(&id, cmd)).await?))
        }
    };
}
macro_rules! worker_command {
    ($handler:ident,$command:ty,$method:ident) => {
        async fn $handler(
            State(s): State<HttpState>,
            body: Result<Json<$command>, JsonRejection>,
        ) -> Result<impl IntoResponse, ApiError> {
            let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
            Ok(Json(blocking(move || s.app.$method(cmd)).await?))
        }
    };
}
case_command!(
    case_correlation,
    crate::correlation::RelationCommand,
    correlate_case
);
case_command!(request_repair, crate::repair::RequestRepair, request_repair);
case_command!(challenge, crate::challenges::ChallengeCommand, challenge);
case_command!(case_gate, crate::challenges::GateCommand, record_gate);
case_command!(
    escalation_resolve,
    crate::notifications::ResolveEscalation,
    resolve_escalation
);
case_command!(
    delivery_reconcile,
    crate::notifications::ReconcileDelivery,
    reconcile_delivery
);
case_command!(
    declare_incident,
    crate::incidents::DeclareIncident,
    declare_incident
);
case_command!(
    incident_operation,
    crate::incident_ops::IncidentOperation,
    operate_incident
);
case_command!(
    incident_artifact,
    crate::incident_artifacts::ArtifactCommand,
    record_incident_artifact
);
case_command!(team_message, PostMessage, post_team_message);
case_command!(team_message_ack, AcknowledgeMessage, acknowledge_message);
case_command!(team_handoff, OfferHandoff, offer_handoff);
case_command!(team_accept, AcceptHandoff, accept_handoff);
case_command!(
    team_escalation_ack,
    AcknowledgeEscalation,
    acknowledge_escalation
);
worker_command!(team_claim, ClaimWork, claim_work);
worker_command!(team_heartbeat, RunCommand, heartbeat);
worker_command!(team_progress, ReportProgress, report_progress);
worker_command!(team_block, ReportBlock, report_block);
worker_command!(team_finish, RunCommand, finish_fake);
async fn team_inbox(State(s): State<HttpState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.escalation_inbox()).await?))
}
async fn list(State(s): State<HttpState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.list_cases()).await?))
}
async fn create(
    State(s): State<HttpState>,
    body: Result<Json<CreateCase>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok((
        StatusCode::CREATED,
        Json(blocking(move || s.app.create_case(cmd)).await?),
    ))
}
async fn snapshot(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.snapshot(&id)).await?))
}
async fn rename(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<RenameCase>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(blocking(move || s.app.rename_case(&id, cmd)).await?))
}
async fn record_evidence(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<RecordEvidence>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok((
        StatusCode::CREATED,
        Json(blocking(move || s.app.record_evidence(&id, cmd)).await?),
    ))
}
async fn record_disposition(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<RecordDisposition>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(
        blocking(move || s.app.record_disposition(&id, cmd)).await?,
    ))
}
async fn reconsider(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<ReconsiderCase>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(
        blocking(move || s.app.reconsider_case(&id, cmd)).await?,
    ))
}
async fn evidence(
    State(s): State<HttpState>,
    Path(digest): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        blocking(move || s.app.evidence(&digest)).await?,
    ))
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: i64,
}
async fn stream(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    Query(query): Query<Cursor>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let mut cursor = match headers.get("last-event-id") {
        Some(v) => v
            .to_str()
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| ApiError(Error::Invalid("invalid Last-Event-ID".into())))?,
        None => query.after,
    };
    let app = s.app.clone();
    let case_id = id.clone();
    blocking(move || app.timeline(&case_id, cursor)).await?;
    let stream = async_stream::stream! {
        loop {
            let app = s.app.clone(); let id = id.clone();
            match blocking(move||app.timeline(&id,cursor)).await {
                Ok(events) => for event in events {
                    cursor = event.cursor;
                    yield Ok::<_,Infallible>(SseEvent::default().event("timeline").id(cursor.to_string()).json_data(&event).unwrap());
                },
                Err(_) => { yield Ok(SseEvent::default().event("unavailable").data("Reload the persisted snapshot")); break; }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10))))
}

async fn sentry_intake(
    State(s): State<HttpState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let signature = headers
        .get("sentry-hook-signature")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let id = blocking(move || s.app.receive_sentry(&body, &signature)).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"delivery_id":id})),
    ))
}
async fn source_health(State(s): State<HttpState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.sentry_health()).await?))
}
async fn incident_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        blocking(move || s.app.incident_markdown(&id)).await?,
    ))
}

async fn qualification_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || s.app.qualification_markdown(&id)).await?,
    ))
}

async fn repair_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || s.app.repair_markdown(&id)).await?,
    ))
}
