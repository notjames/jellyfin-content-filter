pub mod analysis;
pub mod db;

use analysis::{AnalysisConfig, JobManager};
use axum::{
  Json, Router,
  extract::{Path, Request, State},
  http::{HeaderMap, StatusCode},
  middleware::{Next, from_fn_with_state},
  response::{IntoResponse, Response},
  routing::{get, post},
};
use db::{AnalysisJob, Repository, RepositoryError};
use filter_core::{
  Candidate, CandidateStatus, CategorySettings, Evidence, PlaybackConfig, PlaybackInterval, Policy,
  PublishedTimeline, ReviewAction, ReviewEdit, apply_policy, media_fingerprint,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tower_http::limit::RequestBodyLimitLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState
{
  pub repository: Repository,
  pub jobs: JobManager,
  auth_token: Option<Arc<str>>,
}

impl AppState
{
  pub fn new(repository: Repository,
             analysis: AnalysisConfig,
             concurrency: usize,
             auth_token: Option<String>)
             -> Self
  {
    let jobs = JobManager::new(repository.clone(), analysis, concurrency);
    Self { repository, jobs, auth_token: auth_token.map(Arc::from) }
  }
}

pub fn app(state: AppState, max_request_bytes: usize) -> Router
{
  Router::new().route("/health", get(health))
               .route("/analyze", post(enqueue_analysis))
               .route("/analyze/{id}", get(analysis_status))
               .route("/media/{media_id}/evidence", get(media_evidence))
               .route("/media/{media_id}/candidates", get(media_candidates))
               .route("/config/{media_id}/{user_id}", get(playback_config))
               .route("/viewer/{user_id}", axum::routing::put(put_viewer_settings))
               .route("/title/{user_id}/{media_id}", axum::routing::put(put_title_settings))
               .route("/candidates/{id}/approve", post(approve_candidate))
               .route("/candidates/{id}/reject", post(reject_candidate))
               .route("/candidates/{id}/edit", post(edit_candidate))
               .route("/policies", get(list_policies).post(create_policy))
               .route("/policies/{id}", get(get_policy).put(update_policy).delete(delete_policy))
               .route("/timeline/{media_id}/{policy_id}", get(get_timeline).post(publish_timeline))
               .layer(RequestBodyLimitLayer::new(max_request_bytes.max(1024)))
               .layer(from_fn_with_state(state.clone(), authenticate))
               .with_state(state)
}

#[derive(Debug)]
struct ApiError
{
  status: StatusCode,
  message: String,
}

impl ApiError
{
  fn bad_request(message: impl Into<String>) -> Self
  {
    Self { status: StatusCode::BAD_REQUEST, message: message.into() }
  }
}

impl From<RepositoryError> for ApiError
{
  fn from(value: RepositoryError) -> Self
  {
    let status = match value
    {
      RepositoryError::NotFound(_) => StatusCode::NOT_FOUND,
      RepositoryError::InvalidInterval | RepositoryError::Json(_) => StatusCode::BAD_REQUEST,
      RepositoryError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    Self { status, message: value.to_string() }
  }
}

impl IntoResponse for ApiError
{
  fn into_response(self) -> Response
  {
    (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
  }
}

async fn authenticate(State(state): State<AppState>,
                      headers: HeaderMap,
                      request: Request,
                      next: Next)
                      -> Result<Response, StatusCode>
{
  let Some(expected) = &state.auth_token
  else
  {
    return Ok(next.run(request).await);
  };
  let bearer = headers.get("authorization")
                      .and_then(|value| value.to_str().ok())
                      .and_then(|value| value.strip_prefix("Bearer "));
  let local = headers.get("x-filter-token").and_then(|value| value.to_str().ok());
  if bearer.or(local)
           .is_some_and(|provided| constant_time_eq(provided.as_bytes(), expected.as_bytes()))
  {
    Ok(next.run(request).await)
  }
  else
  {
    Err(StatusCode::UNAUTHORIZED)
  }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool
{
  if left.len() != right.len()
  {
    return false;
  }
  left.iter().zip(right).fold(0_u8, |difference, (left, right)| difference | (left ^ right)) == 0
}

#[derive(Serialize)]
struct Health
{
  status: &'static str,
}

async fn health() -> Json<Health>
{
  Json(Health { status: "ok" })
}

#[derive(Deserialize)]
struct AnalyzeRequest
{
  media_id: String,
  media_path: PathBuf,
}

async fn enqueue_analysis(State(state): State<AppState>,
                          Json(request): Json<AnalyzeRequest>)
                          -> Result<(StatusCode, Json<AnalysisJob>), ApiError>
{
  if request.media_id.trim().is_empty()
  {
    return Err(ApiError::bad_request("media_id must not be empty"));
  }
  let job = state.jobs.enqueue(request.media_id.trim(), &request.media_path)?;
  Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn analysis_status(State(state): State<AppState>,
                         Path(id): Path<Uuid>)
                         -> Result<Json<AnalysisJob>, ApiError>
{
  Ok(Json(state.repository.get_job(id)?))
}

async fn media_evidence(State(state): State<AppState>,
                        Path(media_id): Path<String>)
                        -> Result<Json<Vec<Evidence>>, ApiError>
{
  Ok(Json(state.repository.evidence_for_media(&media_id)?))
}

async fn media_candidates(State(state): State<AppState>,
                          Path(media_id): Path<String>)
                          -> Result<Json<Vec<Candidate>>, ApiError>
{
  Ok(Json(state.repository.candidates_for_media(&media_id)?))
}

async fn playback_config(State(state): State<AppState>,
                         Path((media_id, user_id)): Path<(String, String)>)
                         -> Result<Json<PlaybackConfig>, ApiError>
{
  validate_identifier("media_id", &media_id)?;
  validate_identifier("user_id", &user_id)?;
  let viewer = state.repository.viewer_settings(&user_id)?;
  let title = state.repository.title_settings(&user_id, &media_id)?;
  let candidates = state.repository.approved_candidates_for_media(&media_id)?;
  let mut intervals: Vec<_> =
    candidates.into_iter()
              .map(|candidate| {
                PlaybackInterval { start: candidate.interval.start_ms as f64 / 1000.0,
                                   end: candidate.interval.end_ms as f64 / 1000.0,
                                   category: candidate.interval.category.as_str().to_owned() }
              })
              .collect();
  if intervals.iter().any(|interval| !interval.is_valid())
  {
    return Err(ApiError { status: StatusCode::INTERNAL_SERVER_ERROR,
                          message: "stored candidate contains an invalid interval".into() });
  }
  intervals.sort_by(|left, right| {
             left.start
                 .total_cmp(&right.start)
                 .then_with(|| left.end.total_cmp(&right.end))
                 .then_with(|| left.category.cmp(&right.category))
           });
  Ok(Json(PlaybackConfig { viewer, title, intervals }))
}

async fn put_viewer_settings(State(state): State<AppState>,
                             Path(user_id): Path<String>,
                             Json(settings): Json<CategorySettings>)
                             -> Result<Json<CategorySettings>, ApiError>
{
  validate_identifier("user_id", &user_id)?;
  validate_settings(&settings)?;
  state.repository.put_viewer_settings(&user_id, &settings)?;
  Ok(Json(settings))
}

async fn put_title_settings(State(state): State<AppState>,
                            Path((user_id, media_id)): Path<(String, String)>,
                            Json(settings): Json<CategorySettings>)
                            -> Result<Json<CategorySettings>, ApiError>
{
  validate_identifier("user_id", &user_id)?;
  validate_identifier("media_id", &media_id)?;
  validate_settings(&settings)?;
  state.repository.put_title_settings(&user_id, &media_id, &settings)?;
  Ok(Json(settings))
}

fn validate_settings(settings: &CategorySettings) -> Result<(), ApiError>
{
  if settings.has_valid_categories()
  {
    Ok(())
  }
  else
  {
    Err(ApiError::bad_request("categories contains an unsupported category"))
  }
}

fn validate_identifier(name: &str, value: &str) -> Result<(), ApiError>
{
  if value.trim().is_empty()
  {
    Err(ApiError::bad_request(format!("{name} must not be empty")))
  }
  else
  {
    Ok(())
  }
}

#[derive(Default, Deserialize)]
struct ReviewRequest
{
  reviewer: Option<String>,
  edit: Option<ReviewEdit>,
}

async fn approve_candidate(State(state): State<AppState>,
                           Path(id): Path<Uuid>,
                           body: Option<Json<ReviewRequest>>)
                           -> Result<Json<Candidate>, ApiError>
{
  let request = body.map(|Json(value)| value).unwrap_or_default();
  Ok(Json(state.repository.review_candidate(id, ReviewAction::Approve, None, request.reviewer)?))
}

async fn reject_candidate(State(state): State<AppState>,
                          Path(id): Path<Uuid>,
                          body: Option<Json<ReviewRequest>>)
                          -> Result<Json<Candidate>, ApiError>
{
  let request = body.map(|Json(value)| value).unwrap_or_default();
  Ok(Json(state.repository.review_candidate(id, ReviewAction::Reject, None, request.reviewer)?))
}

async fn edit_candidate(State(state): State<AppState>,
                        Path(id): Path<Uuid>,
                        Json(request): Json<ReviewRequest>)
                        -> Result<Json<Candidate>, ApiError>
{
  let edit = request.edit.ok_or_else(|| ApiError::bad_request("edit is required"))?;
  Ok(Json(state.repository
               .review_candidate(id, ReviewAction::Edit, Some(edit), request.reviewer)?))
}

async fn list_policies(State(state): State<AppState>) -> Result<Json<Vec<Policy>>, ApiError>
{
  Ok(Json(state.repository.policies()?))
}

async fn create_policy(State(state): State<AppState>,
                       Json(policy): Json<Policy>)
                       -> Result<(StatusCode, Json<Policy>), ApiError>
{
  validate_policy(&policy)?;
  state.repository.put_policy(&policy)?;
  Ok((StatusCode::CREATED, Json(policy)))
}

async fn get_policy(State(state): State<AppState>,
                    Path(id): Path<Uuid>)
                    -> Result<Json<Policy>, ApiError>
{
  Ok(Json(state.repository.policy(id)?))
}

async fn update_policy(State(state): State<AppState>,
                       Path(id): Path<Uuid>,
                       Json(mut policy): Json<Policy>)
                       -> Result<Json<Policy>, ApiError>
{
  policy.id = id;
  validate_policy(&policy)?;
  state.repository.put_policy(&policy)?;
  Ok(Json(policy))
}

async fn delete_policy(State(state): State<AppState>,
                       Path(id): Path<Uuid>)
                       -> Result<StatusCode, ApiError>
{
  state.repository.delete_policy(id)?;
  Ok(StatusCode::NO_CONTENT)
}

fn validate_policy(policy: &Policy) -> Result<(), ApiError>
{
  if policy.name.trim().is_empty()
  {
    Err(ApiError::bad_request("policy name must not be empty"))
  }
  else
  {
    Ok(())
  }
}

async fn get_timeline(State(state): State<AppState>,
                      Path((media_id, policy_id)): Path<(String, Uuid)>)
                      -> Result<Json<PublishedTimeline>, ApiError>
{
  Ok(Json(state.repository.timeline(&media_id, policy_id)?))
}

#[derive(Deserialize)]
struct PublishRequest
{
  media_path: PathBuf,
}

async fn publish_timeline(State(state): State<AppState>,
                          Path((media_id, policy_id)): Path<(String, Uuid)>,
                          Json(request): Json<PublishRequest>)
                          -> Result<Json<PublishedTimeline>, ApiError>
{
  let policy = state.repository.policy(policy_id)?;
  let candidates = state.repository.candidates_for_media(&media_id)?;
  let intervals: Vec<_> =
    candidates.into_iter()
              .filter(|candidate| candidate.status == CandidateStatus::Approved)
              .map(|candidate| candidate.interval)
              .collect();
  let fingerprint = media_fingerprint(&request.media_path).map_err(|error| {
                      ApiError::bad_request(format!("cannot fingerprint media: {error}"))
                    })?;
  let revision = state.repository
                      .timeline(&media_id, policy_id)
                      .map(|timeline| timeline.revision + 1)
                      .unwrap_or(1);
  let timeline = PublishedTimeline { media_id,
                                     policy_id,
                                     media_fingerprint: fingerprint,
                                     revision,
                                     intervals: apply_policy(&intervals, &policy) };
  state.repository.put_timeline(&timeline)?;
  Ok(Json(timeline))
}

#[cfg(test)]
mod tests
{
  use super::*;
  use axum::body::{Body, to_bytes};
  use filter_core::{Action, Category, Interval, PolicyRule, Severity};
  use http::Request as HttpRequest;
  use tower::ServiceExt;

  fn test_app(token: Option<String>) -> (Router, tempfile::TempDir)
  {
    let dir = tempfile::tempdir().unwrap();
    let repository = Repository::open(dir.path().join("api.db")).unwrap();
    let state = AppState::new(repository, AnalysisConfig::default(), 1, token);
    (app(state, 4096), dir)
  }

  #[tokio::test]
  async fn health_and_auth_work()
  {
    let (router, _dir) = test_app(Some("secret".into()));
    let unauthorized = router.clone()
                             .oneshot(HttpRequest::get("/health").body(Body::empty()).unwrap())
                             .await
                             .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let authorized = router.oneshot(HttpRequest::get("/health").header("authorization",
                                                                       "Bearer secret")
                                                               .body(Body::empty())
                                                               .unwrap())
                           .await
                           .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn policy_crud_round_trip()
  {
    let (router, _dir) = test_app(None);
    let policy = Policy { id: Uuid::new_v4(),
                          name: "default".into(),
                          rules: vec![PolicyRule { category: Category::Profanity,
                                                   minimum_severity: Severity::Low,
                                                   action: Action::Mute,
                                                   enabled: true }] };
    let response = router
            .clone()
            .oneshot(
                HttpRequest::post("/policies")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&policy).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response =
      router.oneshot(HttpRequest::get(format!("/policies/{}", policy.id)).body(Body::empty())
                                                                         .unwrap())
            .await
            .unwrap();
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let returned: Policy = serde_json::from_slice(&body).unwrap();
    assert_eq!(returned, policy);
  }

  #[tokio::test]
  async fn playback_contract_persists_settings_and_returns_only_approved_intervals()
  {
    let dir = tempfile::tempdir().unwrap();
    let repository = Repository::open(dir.path().join("api.db")).unwrap();
    for (status, start_ms) in [(CandidateStatus::Approved, 1250), (CandidateStatus::Pending, 5000)]
    {
      repository.insert_candidate(&Candidate { id: Uuid::new_v4(),
                                               media_id: "movie".into(),
                                               interval: Interval { start_ms,
                                                                    end_ms: start_ms + 2000,
                                                                    action: Action::Skip,
                                                                    category:
                                                                      Category::Violence,
                                                                    severity: Severity::High },
                                               evidence_ids: vec![],
                                               status })
                .unwrap();
    }
    let router = app(AppState::new(repository, AnalysisConfig::default(), 1, None), 4096);

    let viewer_response =
      router.clone()
            .oneshot(HttpRequest::put("/viewer/user")
                               .header("content-type", "application/json")
                               .body(Body::from(r#"{"categories":{"profanity":"mute"}}"#))
                               .unwrap())
            .await
            .unwrap();
    assert_eq!(viewer_response.status(), StatusCode::OK);
    let title_response =
      router.clone()
            .oneshot(HttpRequest::put("/title/user/movie")
                               .header("content-type", "application/json")
                               .body(Body::from(r#"{"categories":{"language":"skip"}}"#))
                               .unwrap())
            .await
            .unwrap();
    assert_eq!(title_response.status(), StatusCode::OK);

    let response = router.oneshot(HttpRequest::get("/config/movie/user").body(Body::empty())
                                                                        .unwrap())
                         .await
                         .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body,
               serde_json::json!({
                 "viewer": {"categories": {"profanity": "mute"}},
                 "title": {"categories": {"language": "skip"}},
                 "intervals": [{
                   "start": 1.25,
                   "end": 3.25,
                   "category": "violence"
                 }]
               }));
    assert!(body["intervals"][0].get("action").is_none());
  }

  #[tokio::test]
  async fn playback_settings_reject_unknown_categories_and_actions()
  {
    let (router, _dir) = test_app(None);
    let unknown_category =
      router.clone()
            .oneshot(HttpRequest::put("/viewer/user")
                               .header("content-type", "application/json")
                               .body(Body::from(r#"{"categories":{"unknown":"mute"}}"#))
                               .unwrap())
            .await
            .unwrap();
    assert_eq!(unknown_category.status(), StatusCode::BAD_REQUEST);

    let unknown_action =
      router.oneshot(HttpRequest::put("/viewer/user")
                               .header("content-type", "application/json")
                               .body(Body::from(r#"{"categories":{"violence":"hide"}}"#))
                               .unwrap())
            .await
            .unwrap();
    assert!(unknown_action.status().is_client_error());
  }

  #[tokio::test]
  async fn request_limit_is_enforced()
  {
    let (router, _dir) = test_app(None);
    let response = router.oneshot(HttpRequest::post("/policies").header("content-type",
                                                                        "application/json")
                                                                .body(Body::from(vec![b'x'; 5000]))
                                                                .unwrap())
                         .await
                         .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
  }
}
