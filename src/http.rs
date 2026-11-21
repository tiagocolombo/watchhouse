use crate::{application::Application, contracts::*, error::Error};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Extension, MatchedPath, Path, Query, Request, State,
        rejection::JsonRejection,
    },
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
    routing::{get, post},
};
use serde::Deserialize;
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Authentication {
    Local,
    Remote(Arc<Remote>),
}
struct Remote {
    secrets: Arc<dyn crate::access::Secrets>,
    requests: Arc<tokio::sync::Semaphore>,
    streams: Arc<tokio::sync::Semaphore>,
    rate: Mutex<(Instant, u32)>,
}

#[derive(Clone)]
struct HttpState {
    app: Application,
    token: String,
    authority: String,
    authentication: Authentication,
}
struct ApiError(Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.0 {
            Error::Invalid(message) => (StatusCode::BAD_REQUEST, "invalid_command", message),
            Error::Backpressure(message) => {
                (StatusCode::TOO_MANY_REQUESTS, "factory_capacity", message)
            }
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
        let mut response = (
            status,
            Json(serde_json::json!({"code":code,"message":message})),
        )
            .into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("1"));
        }
        response
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
    build_router(app, token, authority, Authentication::Local)
}
pub fn remote_router(
    app: Application,
    authority: String,
    secrets: Arc<dyn crate::access::Secrets>,
) -> crate::error::Result<Router> {
    let p = crate::access::active(&app)?;
    crate::access::keys(&p, secrets.as_ref())?;
    Ok(build_router(
        app,
        String::new(),
        authority,
        Authentication::Remote(Arc::new(Remote {
            secrets,
            requests: Arc::new(tokio::sync::Semaphore::new(64)),
            streams: Arc::new(tokio::sync::Semaphore::new(16)),
            rate: Mutex::new((Instant::now(), 0)),
        })),
    ))
}
fn build_router(
    app: Application,
    token: String,
    authority: String,
    authentication: Authentication,
) -> Router {
    let state = HttpState {
        app,
        token,
        authority,
        authentication,
    };
    let api = Router::new()
        .route("/session", get(session_information))
        .route("/cases", get(list).post(create))
        .route("/cases/{id}", get(snapshot))
        .route("/cases/{id}/rename", post(rename))
        .route("/cases/{id}/restore-reminder", post(restore_reminder))
        .route("/cases/{id}/challenges", post(challenge))
        .route("/cases/{id}/gate", post(case_gate))
        .route("/cases/{id}/correlations", post(case_correlation))
        .route("/cases/{id}/repairs", post(request_repair))
        .route(
            "/cases/{id}/container-qualification",
            post(qualify_container),
        )
        .route(
            "/cases/{id}/model-proposals/adopt",
            post(adopt_model_proposal),
        )
        .route("/cases/{id}/model-reviews/adopt", post(adopt_model_review))
        .route("/cases/{id}/publication-plan", get(publication_plan))
        .route(
            "/cases/{id}/publications",
            get(publications).post(enqueue_publication),
        )
        .route("/cases/{id}/model-jobs", post(request_model))
        .route("/cases/{id}/model-jobs/{job}", get(model_job))
        .route("/cases/{id}/model-manage", post(manage_model))
        .route("/cases/{id}/container-jobs/{job}", get(container_job))
        .route("/cases/{id}/container-manage", post(manage_container))
        .route("/cases/{id}/candidates", post(freeze_candidate))
        .route("/cases/{id}/candidate-checks", post(check_candidate))
        .route("/cases/{id}/candidate-reviews", post(review_candidate))
        .route("/cases/{id}/recovery", post(recovery_command))
        .route("/cases/{id}/operations", post(operation_command))
        .route("/cases/{id}/merges", post(enqueue_merge))
        .route("/cases/{id}/merges/retry", post(retry_merge))
        .route("/cases/{id}/learning", post(learning_command))
        .route("/cases/{id}/knowledge-imports", post(import_hallmark))
        .route("/cases/{id}/hallmark-export", post(export_hallmark))
        .route("/cases/{id}/learning/markdown", get(learning_markdown))
        .route("/cases/{id}/recovery/markdown", get(recovery_markdown))
        .route("/cases/{id}/adapter-trials", post(adapter_trial))
        .route(
            "/cases/{id}/adapter-trials/{trial}",
            get(adapter_trial_status),
        )
        .route(
            "/cases/{id}/adapter-trials/{trial}/events",
            get(adapter_trial_events),
        )
        .route(
            "/cases/{id}/worker-qualification/reconcile",
            post(reconcile_worker_qualification),
        )
        .route(
            "/cases/{id}/worker-qualification/markdown",
            get(worker_qualification_markdown),
        )
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
        .route("/sources/datadog", get(datadog_status))
        .route("/model-profiles", get(model_profiles))
        .route("/worker-routes", get(worker_routes))
        .route("/factory", get(factory_status).post(factory_control))
        .route("/retention", get(retention_status))
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
        .route("/cases/{id}/impact", post(assess_impact))
        .route("/cases/{id}/reconsider", post(reconsider))
        .route("/cases/{id}/events", get(stream))
        .route("/evidence/{digest}", get(evidence))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .nest("/api/v1", api)
        .route("/integrations/sentry", post(sentry_intake))
        .route(
            "/integrations/sentry/{organization}/{project}",
            post(sentry_project_intake),
        )
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
fn failure(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({"code":code,"message":message})),
    )
        .into_response()
}
fn one<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        None
    } else {
        Some(value)
    }
}
fn transport(
    headers: &HeaderMap,
    authority: &str,
    p: &crate::access::Policy,
    keys: &crate::access::Keys,
) -> bool {
    one(headers, "host") == Some(authority)
        && one(headers, "x-forwarded-proto") == Some("https")
        && one(headers, "x-forwarded-host") == p.public_origin.strip_prefix("https://")
        && one(headers, "x-watchhouse-proxy-token").is_some_and(|v| {
            v.len() <= 512 && crate::access::equal(&crate::access::digest(v), &keys.proxy)
        })
        && (!headers.contains_key(header::ORIGIN)
            || one(headers, "origin") == Some(p.public_origin.as_str()))
}
async fn local_boundary(State(s): State<HttpState>, req: Request, next: Next) -> Response {
    let mut slot = None;
    let safe = match &s.authentication {
        Authentication::Local => {
            one(req.headers(), "host") == Some(s.authority.as_str())
                && (!req.headers().contains_key(header::ORIGIN)
                    || one(req.headers(), "origin")
                        == Some(format!("http://{}", s.authority).as_str()))
        }
        Authentication::Remote(runtime) => {
            slot = match runtime.requests.clone().try_acquire_owned() {
                Ok(p) => Some(p),
                Err(_) => {
                    return failure(
                        StatusCode::TOO_MANY_REQUESTS,
                        "remote_busy",
                        "Remote request capacity reached; retry shortly",
                    );
                }
            };
            {
                let Ok(mut rate) = runtime.rate.lock() else {
                    return failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "access_unavailable",
                        "Remote admission unavailable",
                    );
                };
                if rate.0.elapsed() >= Duration::from_secs(1) {
                    *rate = (Instant::now(), 0);
                }
                if rate.1 >= 64 {
                    return failure(
                        StatusCode::TOO_MANY_REQUESTS,
                        "remote_busy",
                        "Remote request rate reached; retry shortly",
                    );
                }
                rate.1 += 1;
            }
            let app = s.app.clone();
            let secrets = runtime.secrets.clone();
            let headers = req.headers().clone();
            let authority = s.authority.clone();
            match blocking(move || {
                let p = crate::access::active(&app)?;
                let keys = crate::access::keys(&p, secrets.as_ref())?;
                Ok(transport(&headers, &authority, &p, &keys))
            })
            .await
            {
                Ok(v) => v,
                Err(_) => {
                    return failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "access_unavailable",
                        "Remote access configuration or credential references unavailable",
                    );
                }
            }
        }
    };
    if !safe {
        return failure(
            StatusCode::FORBIDDEN,
            "unsafe_origin",
            "Untrusted host, proxy transport or origin",
        );
    }
    let mut response = next.run(req).await;
    drop(slot);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; img-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    if matches!(s.authentication, Authentication::Remote(_)) {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000".parse().unwrap(),
        );
    }
    response
}
async fn authenticate(State(s): State<HttpState>, mut req: Request, next: Next) -> Response {
    let supplied = one(req.headers(), "authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);
    let Some(supplied) = supplied else {
        return failure(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "An explicit access credential is required",
        );
    };
    let Authentication::Remote(runtime) = &s.authentication else {
        if !crate::access::equal(
            &crate::access::digest(&supplied),
            &crate::access::digest(&s.token),
        ) {
            return failure(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Enter the local operator token printed by serve",
            );
        }
        req.extensions_mut().insert(crate::access::Session::local());
        return next.run(req).await;
    };
    let app = s.app.clone();
    let secrets = runtime.secrets.clone();
    let headers = req.headers().clone();
    let authority = s.authority.clone();
    let authenticated = blocking(move || {
        let p = crate::access::active(&app)?;
        let keys = crate::access::keys(&p, secrets.as_ref())?;
        if !transport(&headers, &authority, &p, &keys) {
            return Ok(None);
        }
        Ok(crate::access::identify(&p, &keys, &supplied).map(|session| (p, session)))
    })
    .await;
    let (policy, session) = match authenticated {
        Ok(Some(v)) => v,
        Ok(None) => {
            return failure(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Use your configured remote access credential",
            );
        }
        Err(_) => {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "access_unavailable",
                "Remote authentication unavailable",
            );
        }
    };
    let method = req.method().as_str().to_owned();
    let original = req.uri().path().to_owned();
    let path = original
        .strip_prefix("/api/v1")
        .unwrap_or(&original)
        .to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|v| v.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let mut decision = if crate::access::permitted(session.role, &method, &path) {
        "allowed"
    } else {
        "role_denied"
    };
    let mut denial = None;
    let mut body_digest = None;
    if decision != "allowed" {
        denial = Some(failure(
            StatusCode::FORBIDDEN,
            "forbidden",
            "This credential does not grant the requested capability",
        ));
    }
    if denial.is_none() {
        let app = s.app.clone();
        let current = session.clone();
        let resource = path.clone();
        if !blocking(move || current.resource_allowed(&app, &resource))
            .await
            .unwrap_or(false)
        {
            decision = "scope_denied";
            denial = Some(failure(
                StatusCode::FORBIDDEN,
                "scope_denied",
                "This credential does not grant the requested resource",
            ));
        }
    }
    if denial.is_none() && method == "POST" {
        let (parts, body) = req.into_parts();
        let bytes = match tokio::time::timeout(
            Duration::from_secs(10),
            axum::body::to_bytes(body, 32 * 1024),
        )
        .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(_)) => {
                return failure(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "body_limit",
                    "Command exceeds 32 KiB",
                );
            }
            Err(_) => {
                return failure(
                    StatusCode::REQUEST_TIMEOUT,
                    "body_timeout",
                    "Command body timed out",
                );
            }
        };
        body_digest = Some(format!(
            "sha256:{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(&bytes)
        ));
        if !path.ends_with("/hallmark-export") {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) if crate::access::binds_actor(&value, &session.id) => {
                    let app = s.app.clone();
                    let current = session.clone();
                    if !blocking(move || current.body_allowed(&app, &value))
                        .await
                        .unwrap_or(false)
                    {
                        decision = "scope_denied";
                        denial = Some(failure(
                            StatusCode::FORBIDDEN,
                            "scope_denied",
                            "Referenced case is outside this credential's grants",
                        ));
                    }
                }
                Ok(_) => {
                    decision = "identity_denied";
                    denial = Some(failure(
                        StatusCode::FORBIDDEN,
                        "identity_mismatch",
                        "Command actor must match the authenticated principal",
                    ));
                }
                Err(_) => {
                    decision = "invalid_json";
                    denial = Some(failure(
                        StatusCode::BAD_REQUEST,
                        "invalid_command",
                        "Command must contain valid JSON",
                    ));
                }
            }
        }
        req = Request::from_parts(parts, axum::body::Body::from(bytes));
    }
    // Body reads can wait; configuration/key revocation is checked again before dispatch.
    let app = s.app.clone();
    let secrets = runtime.secrets.clone();
    let current = session.clone();
    if !blocking(move || current.current(&app, secrets.as_ref()))
        .await
        .unwrap_or(false)
    {
        return failure(
            StatusCode::UNAUTHORIZED,
            "access_revoked",
            "Access changed while preparing the request; reconnect",
        );
    }
    req.extensions_mut().insert(session.clone());
    if method == "GET" && path == "/session" {
        return next.run(req).await;
    }
    let app = s.app.clone();
    let audit_method = method.clone();
    let audit_path = path.clone();
    let audit_session = session.clone();
    let id = match blocking(move || {
        app.reserve_access(
            &policy,
            &audit_session,
            crate::access::AuditInput {
                method: &audit_method,
                route: &route,
                path: &audit_path,
                body_digest,
                decision,
            },
        )
    })
    .await
    {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let mut response = match denial {
        Some(response) => response,
        None => next.run(req).await,
    };
    let status = response.status().as_u16();
    let app = s.app.clone();
    let request_id = id.clone();
    if blocking(move || app.finish_access(&request_id, status))
        .await
        .is_err()
    {
        eprintln!(
            "Remote access audit completion is unresolved; retained intent must be reconciled"
        );
    }
    response
        .headers_mut()
        .insert("x-watchhouse-request-id", id.parse().unwrap());
    response
}
async fn retention_status(State(s): State<HttpState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.retention_status()).await?))
}
async fn session_information(
    Extension(session): Extension<crate::access::Session>,
) -> Json<serde_json::Value> {
    Json(session.information())
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
case_command!(assess_impact, crate::impact::Assess, assess_impact);
case_command!(
    case_correlation,
    crate::correlation::RelationCommand,
    correlate_case
);
case_command!(
    reconcile_worker_qualification,
    crate::worker_qualification::ReconcileQualification,
    reconcile_worker_qualification
);
case_command!(
    adapter_trial,
    crate::adapter_trials::TrialCommand,
    adapter_trial
);
case_command!(
    recovery_command,
    crate::recovery::RecoveryCommand,
    recovery_command
);
case_command!(
    learning_command,
    crate::learning::LearningCommand,
    learning_command
);
case_command!(request_repair, crate::repair::RequestRepair, request_repair);
case_command!(
    qualify_container,
    crate::real_execution::worker::Qualify,
    qualify_container
);
case_command!(
    freeze_candidate,
    crate::real_execution::pipeline::FreezeCandidate,
    freeze_candidate
);
case_command!(
    check_candidate,
    crate::real_execution::pipeline::RequestCheck,
    request_candidate_check
);
case_command!(
    review_candidate,
    crate::real_execution::pipeline::ReviewCandidate,
    review_candidate
);
case_command!(
    adopt_model_proposal,
    crate::model_runtime::adoption::AdoptProposal,
    adopt_model_proposal
);
case_command!(
    adopt_model_review,
    crate::model_runtime::adoption::AdoptReview,
    adopt_model_review
);
case_command!(
    request_model,
    crate::model_runtime::ops::Request,
    request_model
);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicationQuery {
    candidate: String,
    review: String,
}
async fn publications(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        blocking(move || s.app.publication_statuses(&id)).await?,
    ))
}
async fn enqueue_publication(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<crate::delivery::lifecycle::Enqueue>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(
        blocking(move || Ok(s.app.enqueue_publication(&id, cmd)?.status())).await?,
    ))
}
async fn publication_plan(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    Query(q): Query<PublicationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        blocking(move || s.app.prepare_publication(&id, &q.candidate, &q.review)).await?,
    ))
}
async fn model_job(
    State(s): State<HttpState>,
    Path((id, job)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || s.app.model_job(&id, &job)).await?))
}
async fn manage_model(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<crate::model_runtime::ops::Manage>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(
        s.app
            .manage_model(&id, cmd, &crate::model_runtime::ops::Native)
            .await
            .map_err(ApiError)?,
    ))
}
async fn container_job(
    State(s): State<HttpState>,
    Path((id, job)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        blocking(move || s.app.container_job(&id, &job)).await?,
    ))
}
async fn manage_container(
    State(s): State<HttpState>,
    Path(id): Path<String>,
    body: Result<Json<crate::real_execution::worker::Manage>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(cmd) = body.map_err(|e| ApiError(Error::Invalid(e.body_text())))?;
    Ok(Json(
        s.app
            .manage_container(&id, cmd, &crate::real_execution::worker::Native)
            .await
            .map_err(ApiError)?,
    ))
}
case_command!(
    restore_reminder,
    crate::restore::Reminder,
    reissue_restored_escalation
);
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
async fn list(
    State(s): State<HttpState>,
    Extension(session): Extension<crate::access::Session>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || session.list(&s.app)).await?))
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
    Extension(session): Extension<crate::access::Session>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(move || session.snapshot(&s.app, &id)).await?))
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
    Extension(session): Extension<crate::access::Session>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let permit = match &s.authentication {
        Authentication::Local => None,
        Authentication::Remote(runtime) => {
            Some(runtime.streams.clone().try_acquire_owned().map_err(|_| {
                ApiError(Error::Backpressure(
                    "Remote event stream capacity reached; close an unused view".into(),
                ))
            })?)
        }
    };
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
        let _permit=permit;
        loop {
            if let Authentication::Remote(runtime)=&s.authentication {
                let app=s.app.clone();let secrets=runtime.secrets.clone();let current=session.clone();let case_id=id.clone();
                if !blocking(move||current.current_for_case(&app,secrets.as_ref(),&case_id)).await.unwrap_or(false){yield Ok::<_,Infallible>(SseEvent::default().event("access_revoked").data("Access changed; reconnect with a current credential"));break;}
            }
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
async fn sentry_project_intake(
    State(s): State<HttpState>,
    Path((organization, project)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let signature = headers
        .get("sentry-hook-signature")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let id = blocking(move || {
        s.app
            .receive_sentry_project(&organization, &project, &body, &signature)
    })
    .await?;
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
    Extension(session): Extension<crate::access::Session>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || crate::correlation::render_qualification(&session.snapshot(&s.app, &id)?))
            .await?,
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

async fn worker_qualification_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || s.app.worker_qualification_markdown(&id)).await?,
    ))
}

#[derive(serde::Deserialize)]
struct TrialCursor {
    #[serde(default)]
    after: i64,
}
async fn adapter_trial_events(
    State(s): State<HttpState>,
    Path((id, trial)): Path<(String, String)>,
    Query(cursor): Query<TrialCursor>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        blocking(move || s.app.adapter_trial_events(&id, &trial, cursor.after)).await?,
    ))
}

async fn adapter_trial_status(
    State(s): State<HttpState>,
    Path((id, trial)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        blocking(move || s.app.adapter_trial_status(&id, &trial)).await?,
    ))
}

async fn recovery_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || s.app.recovery_markdown(&id)).await?,
    ))
}

async fn learning_markdown(
    State(s): State<HttpState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        blocking(move || s.app.learning_markdown(&id)).await?,
    ))
}

case_command!(
    import_hallmark,
    crate::hallmark::ImportCommand,
    import_hallmark
);
case_command!(
    export_hallmark,
    crate::hallmark::ExportSelection,
    export_hallmark
);

case_command!(
    operation_command,
    crate::operations::Command,
    operation_command
);

case_command!(enqueue_merge, crate::merging::Enqueue, enqueue_merge);

case_command!(retry_merge, crate::merging::Retry, retry_merge);

async fn datadog_status(State(s): State<HttpState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(blocking(move || s.app.datadog_status()).await?)
            .map_err(|e| ApiError(e.into()))?,
    ))
}

async fn model_profiles(State(s): State<HttpState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(blocking(move || s.app.model_profiles()).await?)
            .map_err(|e| ApiError(e.into()))?,
    ))
}

async fn worker_routes(State(s): State<HttpState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(blocking(move || s.app.worker_routes()).await?)
            .map_err(|e| ApiError(e.into()))?,
    ))
}

async fn factory_status(State(s): State<HttpState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(blocking(move || s.app.factory_status()).await?)
            .map_err(|e| ApiError(e.into()))?,
    ))
}
async fn factory_control(
    State(s): State<HttpState>,
    Json(cmd): Json<crate::factory::Command>,
) -> Result<Json<crate::factory::Receipt>, ApiError> {
    Ok(Json(blocking(move || s.app.control_factory(cmd)).await?))
}
