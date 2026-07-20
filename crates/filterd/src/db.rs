use chrono::Utc;
use filter_core::{
  Candidate, CandidateStatus, CategorySettings, Evidence, FilterProblem, FilterReadiness, Policy,
  PublishedTimeline, Review, ReviewAction, ReviewEdit,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MIGRATIONS: &[&str] =
  &[r#"
    CREATE TABLE jobs (
        id TEXT PRIMARY KEY,
        media_id TEXT NOT NULL,
        media_path TEXT NOT NULL,
        status TEXT NOT NULL,
        error TEXT,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE TABLE evidence (
        id TEXT PRIMARY KEY,
        media_id TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE INDEX evidence_media_idx ON evidence(media_id);
    CREATE TABLE candidates (
        id TEXT PRIMARY KEY,
        media_id TEXT NOT NULL,
        status TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE INDEX candidates_media_idx ON candidates(media_id);
    CREATE TABLE reviews (
        id TEXT PRIMARY KEY,
        candidate_id TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE TABLE policies (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE TABLE timelines (
        media_id TEXT NOT NULL,
        policy_id TEXT NOT NULL,
        revision INTEGER NOT NULL,
        payload TEXT NOT NULL,
        PRIMARY KEY(media_id, policy_id)
    );
    "#,
    r#"
    CREATE TABLE viewer_settings (
        user_id TEXT PRIMARY KEY,
        payload TEXT NOT NULL
    );
    CREATE TABLE title_settings (
        user_id TEXT NOT NULL,
        media_id TEXT NOT NULL,
        payload TEXT NOT NULL,
        PRIMARY KEY(user_id, media_id)
    );
    "#,
    r#"
    CREATE TABLE media_status (
        media_id TEXT PRIMARY KEY,
        media_path TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        status TEXT NOT NULL,
        problems TEXT NOT NULL,
        analyzed_at_ms INTEGER NOT NULL
    );
    "#];

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError
{
  #[error("database error: {0}")]
  Sql(#[from] rusqlite::Error),
  #[error("invalid stored data: {0}")]
  Json(#[from] serde_json::Error),
  #[error("{0} not found")]
  NotFound(&'static str),
  #[error("invalid interval")]
  InvalidInterval,
}

pub type Result<T> = std::result::Result<T, RepositoryError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus
{
  Queued,
  Running,
  Complete,
  Failed,
  Skipped,
}

impl JobStatus
{
  fn as_str(self) -> &'static str
  {
    match self
    {
      Self::Queued => "queued",
      Self::Running => "running",
      Self::Complete => "complete",
      Self::Failed => "failed",
      Self::Skipped => "skipped",
    }
  }

  fn parse(value: &str) -> Self
  {
    match value
    {
      "running" => Self::Running,
      "complete" => Self::Complete,
      "failed" => Self::Failed,
      "skipped" => Self::Skipped,
      _ => Self::Queued,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob
{
  pub id: Uuid,
  pub media_id: String,
  pub media_path: PathBuf,
  pub status: JobStatus,
  pub error: Option<String>,
  pub created_at_ms: i64,
  pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAnalysisState
{
  pub media_id: String,
  pub media_path: PathBuf,
  pub fingerprint: String,
  pub status: FilterReadiness,
  pub problems: Vec<FilterProblem>,
  pub analyzed_at_ms: i64,
}

fn map_job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnalysisJob>
{
  Ok(AnalysisJob { id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil()),
                   media_id: row.get(1)?,
                   media_path: PathBuf::from(row.get::<_, String>(2)?),
                   status: JobStatus::parse(&row.get::<_, String>(3)?),
                   error: row.get(4)?,
                   created_at_ms: row.get(5)?,
                   updated_at_ms: row.get(6)? })
}

fn filter_status(status: FilterReadiness) -> &'static str
{
  match status
  {
    FilterReadiness::Ready => "ready",
    FilterReadiness::Partial => "partial",
    FilterReadiness::Failed => "failed",
    FilterReadiness::Missing => "missing",
  }
}

fn parse_filter_status(value: &str) -> FilterReadiness
{
  match value
  {
    "ready" => FilterReadiness::Ready,
    "partial" => FilterReadiness::Partial,
    "failed" => FilterReadiness::Failed,
    _ => FilterReadiness::Missing,
  }
}

#[derive(Clone)]
pub struct Repository
{
  path: PathBuf,
}

impl Repository
{
  pub fn open(path: impl AsRef<Path>) -> Result<Self>
  {
    let repository = Self { path: path.as_ref().to_owned() };
    let mut connection = repository.connect()?;
    connection.execute_batch(
                             "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at_ms INTEGER NOT NULL
             );",
    )?;
    for (index, migration) in MIGRATIONS.iter().enumerate()
    {
      let version = index as i64 + 1;
      let applied: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM schema_migrations \
                                                WHERE version=?1)",
                                               [version],
                                               |row| row.get(0))?;
      if !applied
      {
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration)?;
        transaction.execute("INSERT INTO schema_migrations(version, applied_at_ms) VALUES(?1, \
                             ?2)",
                            params![version, now_ms()])?;
        transaction.commit()?;
      }
    }
    Ok(repository)
  }

  fn connect(&self) -> Result<Connection>
  {
    let connection = Connection::open(&self.path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(connection)
  }

  pub fn create_job(&self, media_id: &str, media_path: &Path) -> Result<AnalysisJob>
  {
    let job = AnalysisJob { id: Uuid::new_v4(),
                            media_id: media_id.to_owned(),
                            media_path: media_path.to_owned(),
                            status: JobStatus::Queued,
                            error: None,
                            created_at_ms: now_ms(),
                            updated_at_ms: now_ms() };
    self.connect()?.execute(
                             "INSERT INTO jobs(id, media_id, media_path, status, \
                              created_at_ms, updated_at_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                             params![
      job.id.to_string(),
      job.media_id,
      job.media_path.to_string_lossy(),
      job.status.as_str(),
      job.created_at_ms,
      job.updated_at_ms
    ],
    )?;
    Ok(job)
  }

  pub fn active_job_for_media(&self, media_id: &str) -> Result<Option<AnalysisJob>>
  {
    self.connect()?
        .query_row(
                   "SELECT id, media_path, status, error, created_at_ms, updated_at_ms
                 FROM jobs
                 WHERE media_id=?1 AND status IN ('queued', 'running')
                 ORDER BY created_at_ms DESC LIMIT 1",
                   [media_id],
                   |row| {
                     Ok(AnalysisJob { id: Uuid::parse_str(&row.get::<_, String>(0)?)
                                        .unwrap_or_else(|_| Uuid::nil()),
                                      media_id: media_id.to_owned(),
                                      media_path: PathBuf::from(row.get::<_, String>(1)?),
                                      status: JobStatus::parse(&row.get::<_, String>(2)?),
                                      error: row.get(3)?,
                                      created_at_ms: row.get(4)?,
                                      updated_at_ms: row.get(5)? })
                   },
        )
        .optional()
        .map_err(Into::into)
  }

  pub fn job_counts(&self) -> Result<Vec<(String, i64)>>
  {
    let connection = self.connect()?;
    let mut statement =
      connection.prepare("SELECT status, COUNT(*) FROM jobs GROUP BY status ORDER BY status")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    let mut values = Vec::new();
    for row in rows
    {
      values.push(row?);
    }
    Ok(values)
  }

  pub fn list_jobs(&self, status: Option<&str>, limit: usize) -> Result<Vec<AnalysisJob>>
  {
    let connection = self.connect()?;
    let limit = limit.clamp(1, 500) as i64;
    let mut values = Vec::new();
    if let Some(status) = status
    {
      let mut statement = connection.prepare(
                                              "SELECT id, media_id, media_path, status, error, created_at_ms, updated_at_ms
             FROM jobs WHERE status=?1 ORDER BY updated_at_ms DESC LIMIT ?2")?;
      let rows = statement.query_map(params![status, limit], map_job_row)?;
      for row in rows
      {
        values.push(row?);
      }
    }
    else
    {
      let mut statement = connection.prepare(
                                              "SELECT id, media_id, media_path, status, error, created_at_ms, updated_at_ms
             FROM jobs ORDER BY updated_at_ms DESC LIMIT ?1")?;
      let rows = statement.query_map([limit], map_job_row)?;
      for row in rows
      {
        values.push(row?);
      }
    }
    Ok(values)
  }

  pub fn get_job(&self, id: Uuid) -> Result<AnalysisJob>
  {
    self.connect()?
        .query_row(
                   "SELECT media_id, media_path, status, error, created_at_ms, updated_at_ms
                 FROM jobs WHERE id=?1",
                   [id.to_string()],
                   |row| {
                     Ok(AnalysisJob { id,
                                      media_id: row.get(0)?,
                                      media_path: PathBuf::from(row.get::<_, String>(1)?),
                                      status: JobStatus::parse(&row.get::<_, String>(2)?),
                                      error: row.get(3)?,
                                      created_at_ms: row.get(4)?,
                                      updated_at_ms: row.get(5)? })
                   },
    )
        .optional()?
        .ok_or(RepositoryError::NotFound("job"))
  }

  pub fn update_job(&self, id: Uuid, status: JobStatus, error: Option<&str>) -> Result<()>
  {
    let changed =
      self.connect()?
          .execute("UPDATE jobs SET status=?2, error=?3, updated_at_ms=?4 WHERE id=?1",
                   params![id.to_string(), status.as_str(), error, now_ms()])?;
    if changed == 0
    {
      return Err(RepositoryError::NotFound("job"));
    }
    Ok(())
  }

  /// Move orphaned `running` rows back to `queued` after a process restart.
  pub fn requeue_orphaned_running(&self) -> Result<usize>
  {
    let changed =
      self.connect()?
          .execute("UPDATE jobs SET status='queued', error=NULL, updated_at_ms=?1 \
                    WHERE status='running'",
                   params![now_ms()])?;
    Ok(changed as usize)
  }

  /// Keep one queued job per media_id (oldest) and mark the rest skipped.
  pub fn collapse_duplicate_queued(&self) -> Result<usize>
  {
    let changed = self.connect()?.execute(
                                          "UPDATE jobs
             SET status='skipped',
                 error='duplicate queued job',
                 updated_at_ms=?1
             WHERE status='queued'
               AND rowid NOT IN (
                 SELECT MIN(rowid)
                 FROM jobs
                 WHERE status='queued'
                 GROUP BY media_id
               )",
                                          params![now_ms()],
    )?;
    Ok(changed as usize)
  }

  pub fn has_queued_jobs(&self) -> Result<bool>
  {
    let exists: bool =
      self.connect()?
          .query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE status='queued')",
                     [],
                     |row| row.get(0))?;
    Ok(exists)
  }

  /// Atomically claim the next queued job (FIFO) and mark it running.
  pub fn claim_next_queued(&self) -> Result<Option<AnalysisJob>>
  {
    let mut connection = self.connect()?;
    let transaction = connection.transaction()?;
    let job = transaction.query_row(
                                     "SELECT id, media_id, media_path, status, error, \
                                      created_at_ms, updated_at_ms
             FROM jobs
             WHERE status='queued'
             ORDER BY created_at_ms ASC, rowid ASC
             LIMIT 1",
                                     [],
                                     map_job_row,
    )
                            .optional()?;
    let Some(mut job) = job
    else
    {
      transaction.commit()?;
      return Ok(None);
    };
    let updated_at = now_ms();
    transaction.execute("UPDATE jobs SET status='running', error=NULL, updated_at_ms=?2 \
                         WHERE id=?1 AND status='queued'",
                        params![job.id.to_string(), updated_at])?;
    transaction.commit()?;
    job.status = JobStatus::Running;
    job.updated_at_ms = updated_at;
    job.error = None;
    Ok(Some(job))
  }

  pub fn insert_evidence(&self, evidence: &Evidence) -> Result<()>
  {
    insert_payload(&self.connect()?,
                   "INSERT OR REPLACE INTO evidence(id, media_id, payload) VALUES(?1, ?2, ?3)",
                   evidence.id,
                   &evidence.media_id,
                   evidence)
  }

  pub fn evidence_for_media(&self, media_id: &str) -> Result<Vec<Evidence>>
  {
    payloads_for_media(&self.connect()?,
                       "SELECT payload FROM evidence WHERE media_id=?1 ORDER BY id",
                       media_id)
  }

  pub fn insert_candidate(&self, candidate: &Candidate) -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT OR REPLACE INTO candidates(id, media_id, status, payload)
             VALUES(?1, ?2, ?3, ?4)",
                             params![
      candidate.id.to_string(),
      candidate.media_id,
      candidate_status(candidate.status),
      serde_json::to_string(candidate)?
    ],
    )?;
    Ok(())
  }

  pub fn candidates_for_media(&self, media_id: &str) -> Result<Vec<Candidate>>
  {
    payloads_for_media(&self.connect()?,
                       "SELECT payload FROM candidates WHERE media_id=?1 ORDER BY id",
                       media_id)
  }

  pub fn approved_candidates_for_media(&self, media_id: &str) -> Result<Vec<Candidate>>
  {
    payloads_for_media(&self.connect()?,
                       "SELECT payload FROM candidates
                        WHERE media_id=?1 AND status='approved' ORDER BY id",
                       media_id)
  }

  pub fn put_media_state(&self, state: &MediaAnalysisState) -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT INTO media_status(media_id, media_path, fingerprint, status, problems, analyzed_at_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(media_id) DO UPDATE SET
               media_path=excluded.media_path,
               fingerprint=excluded.fingerprint,
               status=excluded.status,
               problems=excluded.problems,
               analyzed_at_ms=excluded.analyzed_at_ms",
                             params![
      state.media_id,
      state.media_path.to_string_lossy(),
      state.fingerprint,
      filter_status(state.status),
      serde_json::to_string(&state.problems)?,
      state.analyzed_at_ms
    ],
    )?;
    Ok(())
  }

  pub fn media_state(&self, media_id: &str) -> Result<Option<MediaAnalysisState>>
  {
    self.connect()?
        .query_row(
                   "SELECT media_path, fingerprint, status, problems, analyzed_at_ms
                 FROM media_status WHERE media_id=?1",
                   [media_id],
                   |row| {
                     Ok((row.get::<_, String>(0)?,
                         row.get::<_, String>(1)?,
                         row.get::<_, String>(2)?,
                         row.get::<_, String>(3)?,
                         row.get::<_, i64>(4)?))
                   },
        )
        .optional()?
        .map(|(media_path, fingerprint, status, problems, analyzed_at_ms)| {
          Ok(MediaAnalysisState { media_id: media_id.to_owned(),
                                  media_path: PathBuf::from(media_path),
                                  fingerprint,
                                  status: parse_filter_status(&status),
                                  problems: serde_json::from_str(&problems)?,
                                  analyzed_at_ms })
        })
        .transpose()
  }

  pub fn list_problem_media(&self) -> Result<Vec<MediaAnalysisState>>
  {
    let connection = self.connect()?;
    let mut statement = connection.prepare(
                                            "SELECT media_id, media_path, fingerprint, status, problems, analyzed_at_ms
             FROM media_status
             WHERE status IN ('partial', 'failed', 'missing')
             ORDER BY analyzed_at_ms DESC")?;
    let rows = statement.query_map([], |row| {
                          Ok((row.get::<_, String>(0)?,
                              row.get::<_, String>(1)?,
                              row.get::<_, String>(2)?,
                              row.get::<_, String>(3)?,
                              row.get::<_, String>(4)?,
                              row.get::<_, i64>(5)?))
                        })?;
    let mut values = Vec::new();
    for row in rows
    {
      let (media_id, media_path, fingerprint, status, problems, analyzed_at_ms) = row?;
      values.push(MediaAnalysisState { media_id,
                                       media_path: PathBuf::from(media_path),
                                       fingerprint,
                                       status: parse_filter_status(&status),
                                       problems: serde_json::from_str(&problems)?,
                                       analyzed_at_ms });
    }
    Ok(values)
  }

  pub fn put_viewer_settings(&self, user_id: &str, settings: &CategorySettings) -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT INTO viewer_settings(user_id, payload) VALUES(?1, ?2)
             ON CONFLICT(user_id) DO UPDATE SET payload=excluded.payload",
                             params![user_id, serde_json::to_string(settings)?],
    )?;
    Ok(())
  }

  pub fn viewer_settings(&self, user_id: &str) -> Result<CategorySettings>
  {
    let payload: Option<String> =
      self.connect()?
          .query_row("SELECT payload FROM viewer_settings WHERE user_id=?1", [user_id], |row| {
            row.get(0)
          })
          .optional()?;
    payload.map(|value| serde_json::from_str(&value))
           .transpose()
           .map(Option::unwrap_or_default)
           .map_err(Into::into)
  }

  pub fn put_title_settings(&self,
                            user_id: &str,
                            media_id: &str,
                            settings: &CategorySettings)
                            -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT INTO title_settings(user_id, media_id, payload)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(user_id, media_id) DO UPDATE SET payload=excluded.payload",
                             params![user_id, media_id, serde_json::to_string(settings)?],
    )?;
    Ok(())
  }

  pub fn title_settings(&self, user_id: &str, media_id: &str) -> Result<CategorySettings>
  {
    let payload: Option<String> =
      self.connect()?
          .query_row("SELECT payload FROM title_settings WHERE user_id=?1 AND media_id=?2",
                     params![user_id, media_id],
                     |row| row.get(0))
          .optional()?;
    payload.map(|value| serde_json::from_str(&value))
           .transpose()
           .map(Option::unwrap_or_default)
           .map_err(Into::into)
  }

  pub fn review_candidate(&self,
                          candidate_id: Uuid,
                          action: ReviewAction,
                          edit: Option<ReviewEdit>,
                          reviewer: Option<String>)
                          -> Result<Candidate>
  {
    if matches!(action, ReviewAction::Edit)
       && edit.as_ref().is_none_or(|value| value.start_ms >= value.end_ms)
    {
      return Err(RepositoryError::InvalidInterval);
    }
    let mut connection = self.connect()?;
    let transaction = connection.transaction()?;
    let payload: Option<String> = transaction.query_row("SELECT payload FROM candidates WHERE \
                                                         id=?1",
                                                        [candidate_id.to_string()],
                                                        |row| row.get(0))
                                             .optional()?;
    let mut candidate: Candidate =
      serde_json::from_str(&payload.ok_or(RepositoryError::NotFound("candidate"))?)?;
    candidate.status = match action
    {
      ReviewAction::Reject => CandidateStatus::Rejected,
      ReviewAction::Approve | ReviewAction::Edit => CandidateStatus::Approved,
    };
    if let Some(value) = edit.clone()
    {
      candidate.interval = value.into();
    }
    transaction.execute("UPDATE candidates SET status=?2, payload=?3 WHERE id=?1",
                        params![candidate.id.to_string(),
                                candidate_status(candidate.status),
                                serde_json::to_string(&candidate)?])?;
    let review =
      Review { id: Uuid::new_v4(), candidate_id, action, edit, reviewer, created_at_ms: now_ms() };
    transaction.execute("INSERT INTO reviews(id, candidate_id, payload) VALUES(?1, ?2, ?3)",
                        params![review.id.to_string(),
                                candidate_id.to_string(),
                                serde_json::to_string(&review)?])?;
    transaction.commit()?;
    Ok(candidate)
  }

  pub fn put_policy(&self, policy: &Policy) -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT INTO policies(id, name, payload) VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, payload=excluded.payload",
                             params![
      policy.id.to_string(),
      policy.name,
      serde_json::to_string(policy)?
    ],
    )?;
    Ok(())
  }

  pub fn policy(&self, id: Uuid) -> Result<Policy>
  {
    get_payload(&self.connect()?, "SELECT payload FROM policies WHERE id=?1", id, "policy")
  }

  pub fn policies(&self) -> Result<Vec<Policy>>
  {
    payloads(&self.connect()?, "SELECT payload FROM policies ORDER BY name")
  }

  pub fn delete_policy(&self, id: Uuid) -> Result<()>
  {
    let changed = self.connect()?.execute("DELETE FROM policies WHERE id=?1", [id.to_string()])?;
    if changed == 0
    {
      return Err(RepositoryError::NotFound("policy"));
    }
    Ok(())
  }

  pub fn put_timeline(&self, timeline: &PublishedTimeline) -> Result<()>
  {
    self.connect()?.execute(
                             "INSERT INTO timelines(media_id, policy_id, revision, payload)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(media_id, policy_id) DO UPDATE
             SET revision=excluded.revision, payload=excluded.payload",
                             params![
      timeline.media_id,
      timeline.policy_id.to_string(),
      timeline.revision,
      serde_json::to_string(timeline)?
    ],
    )?;
    Ok(())
  }

  pub fn timeline(&self, media_id: &str, policy_id: Uuid) -> Result<PublishedTimeline>
  {
    let payload: Option<String> =
      self.connect()?
          .query_row("SELECT payload FROM timelines WHERE media_id=?1 AND policy_id=?2",
                     params![media_id, policy_id.to_string()],
                     |row| row.get(0))
          .optional()?;
    serde_json::from_str(&payload.ok_or(RepositoryError::NotFound("timeline"))?).map_err(Into::into)
  }
}

fn candidate_status(status: CandidateStatus) -> &'static str
{
  match status
  {
    CandidateStatus::Pending => "pending",
    CandidateStatus::Approved => "approved",
    CandidateStatus::Rejected => "rejected",
  }
}

fn insert_payload<T: Serialize>(connection: &Connection,
                                sql: &str,
                                id: Uuid,
                                media_id: &str,
                                value: &T)
                                -> Result<()>
{
  connection.execute(sql, params![id.to_string(), media_id, serde_json::to_string(value)?])?;
  Ok(())
}

fn get_payload<T: DeserializeOwned>(connection: &Connection,
                                    sql: &str,
                                    id: Uuid,
                                    entity: &'static str)
                                    -> Result<T>
{
  let payload: Option<String> =
    connection.query_row(sql, [id.to_string()], |row| row.get(0)).optional()?;
  serde_json::from_str(&payload.ok_or(RepositoryError::NotFound(entity))?).map_err(Into::into)
}

fn payloads_for_media<T: DeserializeOwned>(connection: &Connection,
                                           sql: &str,
                                           media_id: &str)
                                           -> Result<Vec<T>>
{
  let mut statement = connection.prepare(sql)?;
  let rows = statement.query_map([media_id], |row| row.get::<_, String>(0))?;
  rows.map(|row| {
        let payload = row?;
        serde_json::from_str(&payload).map_err(Into::into)
      })
      .collect()
}

fn payloads<T: DeserializeOwned>(connection: &Connection, sql: &str) -> Result<Vec<T>>
{
  let mut statement = connection.prepare(sql)?;
  let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
  rows.map(|row| {
        let payload = row?;
        serde_json::from_str(&payload).map_err(Into::into)
      })
      .collect()
}

fn now_ms() -> i64
{
  Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests
{
  use super::*;
  use filter_core::{Action, Category, Interval, Severity, ViewerAction};

  #[test]
  fn migrates_and_round_trips_review()
  {
    let dir = tempfile::tempdir().unwrap();
    let repository = Repository::open(dir.path().join("filter.db")).unwrap();
    let candidate = Candidate { id: Uuid::new_v4(),
                                media_id: "movie".into(),
                                interval: Interval { start_ms: 1,
                                                     end_ms: 2,
                                                     action: Action::Mute,
                                                     category: Category::Profanity,
                                                     severity: Severity::Medium },
                                evidence_ids: vec![],
                                status: CandidateStatus::Pending };
    repository.insert_candidate(&candidate).unwrap();
    let approved =
      repository.review_candidate(candidate.id, ReviewAction::Approve, None, None).unwrap();
    assert_eq!(approved.status, CandidateStatus::Approved);
    assert_eq!(repository.candidates_for_media("movie").unwrap()[0].status,
               CandidateStatus::Approved);
  }

  #[test]
  fn migrates_and_persists_viewer_and_title_settings()
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("filter.db");
    let repository = Repository::open(&path).unwrap();
    let mut viewer = CategorySettings::default();
    viewer.categories.insert("profanity".into(), ViewerAction::Mute);
    let mut title = CategorySettings::default();
    title.categories.insert("violence".into(), ViewerAction::Skip);

    repository.put_viewer_settings("user", &viewer).unwrap();
    repository.put_title_settings("user", "movie", &title).unwrap();

    let reopened = Repository::open(path).unwrap();
    assert_eq!(reopened.viewer_settings("user").unwrap(), viewer);
    assert_eq!(reopened.title_settings("user", "movie").unwrap(), title);
    assert_eq!(reopened.viewer_settings("new-user").unwrap(), CategorySettings::default());
  }

  #[test]
  fn requeues_orphaned_running_and_collapses_duplicate_queued()
  {
    let dir = tempfile::tempdir().unwrap();
    let repository = Repository::open(dir.path().join("filter.db")).unwrap();
    let first = repository.create_job("movie", Path::new("/media/a.mp4")).unwrap();
    let _second = repository.create_job("movie", Path::new("/media/a.mp4")).unwrap();
    let other = repository.create_job("other", Path::new("/media/b.mp4")).unwrap();
    repository.update_job(other.id, JobStatus::Running, None).unwrap();

    assert_eq!(repository.requeue_orphaned_running().unwrap(), 1);
    assert_eq!(repository.collapse_duplicate_queued().unwrap(), 1);

    let counts: std::collections::BTreeMap<_, _> =
      repository.job_counts().unwrap().into_iter().collect();
    assert_eq!(counts.get("queued").copied().unwrap_or(0), 2);
    assert_eq!(counts.get("skipped").copied().unwrap_or(0), 1);
    assert_eq!(counts.get("running").copied().unwrap_or(0), 0);

    let claimed = repository.claim_next_queued().unwrap().unwrap();
    assert_eq!(claimed.id, first.id);
    assert_eq!(claimed.status, JobStatus::Running);
    assert!(repository.has_queued_jobs().unwrap());
    let claimed_other = repository.claim_next_queued().unwrap().unwrap();
    assert_eq!(claimed_other.media_id, "other");
    assert!(!repository.has_queued_jobs().unwrap());
    assert!(repository.claim_next_queued().unwrap().is_none());
  }
}
