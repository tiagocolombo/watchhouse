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
        .route("/cases/{id}/evidence", post(record_evidence))
        .route("/cases/{id}/dispositions", post(record_disposition))
        .route("/cases/{id}/reconsider", post(reconsider))
        .route("/cases/{id}/events", get(stream))
        .route("/evidence/{digest}", get(evidence))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .nest("/api/v1", api)
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
