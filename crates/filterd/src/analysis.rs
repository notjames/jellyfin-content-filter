use crate::db::{AnalysisJob, JobStatus, MediaAnalysisState, Repository};
use filter_core::{
  Action, Candidate, CandidateStatus, Category, Evidence, FilterDetection, FilterDocument,
  FilterProblem, FilterReadiness, Interval, ProfanityConfig, ProfanityMatcher, Severity,
  WhisperOptions, ffmpeg_audio_spec, ffmpeg_frame_sample_spec, ffprobe_spec, media_fingerprint,
  openvino_spec, parse_subtitles, whisper_spec_with_options,
};
use serde::Deserialize;
use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

use crate::scheduling::AnalysisSchedule;

fn log_queue_status(repository: &Repository, event: &str)
{
  match repository.job_counts()
  {
    Ok(counts) =>
    {
      let summary: std::collections::BTreeMap<_, _> = counts.into_iter().collect();
      tracing::info!(event, ?summary, "analysis queue status");
    }
    Err(error) => tracing::warn!(%error, event, "failed to read analysis queue status"),
  }
}

fn log_file_path(path: &Path) -> String
{
  path.display().to_string()
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig
{
  pub ffprobe: PathBuf,
  pub ffmpeg: PathBuf,
  pub whisper_executable: Option<PathBuf>,
  pub whisper_model: Option<PathBuf>,
  pub whisper_options: WhisperOptions,
  pub openvino_runner: Option<PathBuf>,
  pub openvino_model: Option<PathBuf>,
  pub openvino_labels: Option<PathBuf>,
  pub openvino_device: String,
  pub frame_interval_seconds: u32,
  pub profanity: ProfanityConfig,
}

impl Default for AnalysisConfig
{
  fn default() -> Self
  {
    Self { ffprobe: "ffprobe".into(),
           ffmpeg: "ffmpeg".into(),
           whisper_executable: None,
           whisper_model: None,
           whisper_options: WhisperOptions::default(),
           openvino_runner: None,
           openvino_model: None,
           openvino_labels: None,
           openvino_device: "GPU".into(),
           frame_interval_seconds: 5,
           profanity: ProfanityConfig::default() }
  }
}

#[derive(Debug, Clone, Copy)]
pub struct QueueRecoveryStats
{
  pub requeued_running: usize,
  pub collapsed_duplicates: usize,
}

#[derive(Clone)]
pub struct JobManager
{
  repository: Repository,
  config: Arc<AnalysisConfig>,
  schedule: Arc<AnalysisSchedule>,
  permits: Arc<Semaphore>,
  notify: Arc<Notify>,
}

impl JobManager
{
  pub fn new(repository: Repository,
             config: AnalysisConfig,
             schedule: AnalysisSchedule,
             concurrency: usize)
             -> Self
  {
    Self { repository,
           config: Arc::new(config),
           schedule: Arc::new(schedule),
           permits: Arc::new(Semaphore::new(concurrency.max(1))),
           notify: Arc::new(Notify::new()) }
  }

  /// Recover orphaned/duplicate queue rows and start the background dispatcher.
  pub fn start_dispatcher(&self)
  {
    let manager = self.clone();
    tokio::spawn(async move {
      match manager.recover_queue()
      {
        Ok(stats) =>
        {
          tracing::info!(requeued_running = stats.requeued_running,
                         collapsed_duplicates = stats.collapsed_duplicates,
                         "recovered analysis queue");
          log_queue_status(&manager.repository, "after_recovery");
        }
        Err(error) => tracing::error!(%error, "failed to recover analysis queue"),
      }
      manager.schedule.log_startup();
      manager.dispatch_loop().await;
    });
  }

  pub fn recover_queue(&self) -> crate::db::Result<QueueRecoveryStats>
  {
    let requeued_running = self.repository.requeue_orphaned_running()?;
    let collapsed_duplicates = self.repository.collapse_duplicate_queued()?;
    if requeued_running > 0 || collapsed_duplicates > 0 || self.repository.has_queued_jobs()?
    {
      self.notify.notify_one();
    }
    Ok(QueueRecoveryStats { requeued_running, collapsed_duplicates })
  }

  pub fn enqueue(&self, media_id: &str, path: &Path, force: bool) -> crate::db::Result<AnalysisJob>
  {
    if !force
    {
      if let Some(existing) = self.repository.active_job_for_media(media_id)?
      {
        tracing::debug!(media_id,
                        file = %log_file_path(path),
                        job_id = %existing.id,
                        status = ?existing.status,
                        "analysis already active for media");
        self.notify.notify_one();
        return Ok(existing);
      }
      if let Ok(fingerprint) = media_fingerprint(path)
      {
        let sidecar = FilterDocument::sidecar_path(path);
        if let Ok(existing) = FilterDocument::load(&sidecar)
        {
          if existing.media_fingerprint == fingerprint
             && matches!(existing.status, FilterReadiness::Ready | FilterReadiness::Partial)
          {
            let now = chrono::Utc::now().timestamp_millis();
            let _ = self.repository.put_media_state(&MediaAnalysisState {
              media_id: media_id.to_owned(),
              media_path: path.to_owned(),
              fingerprint,
              status: existing.status,
              problems: existing.problems.clone(),
              analyzed_at_ms: existing.analyzed_at_ms,
            });
            tracing::debug!(media_id,
                            file = %log_file_path(path),
                            "skipped analysis; sidecar is current");
            return Ok(AnalysisJob { id: Uuid::nil(),
                                    media_id: media_id.to_owned(),
                                    media_path: path.to_owned(),
                                    status: JobStatus::Skipped,
                                    error: Some("up-to-date filter.json".into()),
                                    created_at_ms: now,
                                    updated_at_ms: now });
          }
        }
      }
    }

    let job = self.repository.create_job(media_id, path)?;
    tracing::debug!(media_id,
                    file = %log_file_path(path),
                    job_id = %job.id,
                    force,
                    "queued analysis job");
    self.notify.notify_one();
    Ok(job)
  }

  async fn dispatch_loop(&self)
  {
    let mut outside_window_logged = false;
    let mut transcoding_logged = false;
    loop
    {
      while matches!(self.repository.has_queued_jobs(), Ok(false))
      {
        self.notify.notified().await;
      }

      while !self.schedule.can_start().await
      {
        if !self.schedule.in_time_window()
        {
          if !outside_window_logged
          {
            tracing::info!("analysis queue waiting for configured time window");
            outside_window_logged = true;
            transcoding_logged = false;
          }
        }
        else if !transcoding_logged
        {
          tracing::info!("analysis queue yielding while Jellyfin is transcoding");
          outside_window_logged = false;
          transcoding_logged = true;
        }
        tokio::time::sleep(self.schedule.poll_interval()).await;
      }
      if outside_window_logged || transcoding_logged
      {
        tracing::info!("analysis queue resuming");
      }
      outside_window_logged = false;
      transcoding_logged = false;

      let permit = match self.permits.clone().acquire_owned().await
      {
        Ok(permit) => permit,
        Err(_) => return,
      };

      let job = match self.repository.claim_next_queued()
      {
        Ok(Some(job)) =>
        {
          tracing::info!(job_id = %job.id,
                         media_id = %job.media_id,
                         file = %log_file_path(&job.media_path),
                         "claimed analysis job");
          job
        }
        Ok(None) =>
        {
          drop(permit);
          continue;
        }
        Err(error) =>
        {
          tracing::warn!(%error, "failed to claim queued analysis job");
          drop(permit);
          tokio::time::sleep(Duration::from_secs(1)).await;
          continue;
        }
      };

      let manager = self.clone();
      tokio::spawn(async move {
        let _permit = permit;
        manager.run_job(job).await;
      });
    }
  }

  async fn run_job(&self, job: AnalysisJob)
  {
    let file = log_file_path(&job.media_path);
    tracing::info!(job_id = %job.id, media_id = %job.media_id, file = %file, "processing file");
    match analyze(&self.repository, &self.config, &job.media_id, &job.media_path).await
    {
      Ok(outcome) =>
      {
        let _ = self.repository.update_job(job.id, JobStatus::Complete, None);
        tracing::info!(job_id = %job.id,
                       media_id = %job.media_id,
                       file = %file,
                       status = ?outcome.status,
                       detections = outcome.detections,
                       sidecar = %outcome.sidecar,
                       "completed file");
        log_queue_status(&self.repository, "after_complete");
      }
      Err(error) =>
      {
        let reason = error.to_string();
        tracing::error!(job_id = %job.id,
                         media_id = %job.media_id,
                         file = %file,
                         reason = %reason,
                         "error processing file");
        let _ = self.repository.put_media_state(&MediaAnalysisState {
          media_id: job.media_id.clone(),
          media_path: job.media_path.clone(),
          fingerprint: String::new(),
          status: FilterReadiness::Failed,
          problems: vec![FilterProblem { code: "analysis_failed".into(),
                                         message: reason.clone() }],
          analyzed_at_ms: chrono::Utc::now().timestamp_millis(),
        });
        let sidecar = FilterDocument::sidecar_path(&job.media_path);
        let failed = FilterDocument { version: FilterDocument::CURRENT_VERSION,
                                      media_id: job.media_id.clone(),
                                      media_fingerprint: String::new(),
                                      analyzed_at_ms: chrono::Utc::now().timestamp_millis(),
                                      status: FilterReadiness::Failed,
                                      problems: vec![FilterProblem { code: "analysis_failed".into(),
                                                                     message: reason.clone() }],
                                      detections: vec![] };
        if let Err(save_error) = failed.save(&sidecar)
        {
          tracing::error!(job_id = %job.id,
                            file = %file,
                            sidecar = %sidecar.display(),
                            reason = %save_error,
                            "failed to write failed-analysis sidecar");
        }
        let _ = self.repository.update_job(job.id, JobStatus::Failed, Some(&reason));
        log_queue_status(&self.repository, "after_failed");
      }
    }
  }
}

struct AnalysisOutcome
{
  status: FilterReadiness,
  detections: usize,
  sidecar: String,
}

async fn analyze(repository: &Repository,
                 config: &AnalysisConfig,
                 media_id: &str,
                 media_path: &Path)
                 -> anyhow::Result<AnalysisOutcome>
{
  let file = log_file_path(media_path);
  if !media_path.is_file()
  {
    anyhow::bail!("media path is not a file");
  }
  let fingerprint = media_fingerprint(media_path)?;
  let sidecar_path = FilterDocument::sidecar_path(media_path);
  if let Ok(existing) = FilterDocument::load(&sidecar_path)
  {
    if existing.media_fingerprint == fingerprint
       && matches!(existing.status, FilterReadiness::Ready | FilterReadiness::Partial)
    {
      repository.put_media_state(&MediaAnalysisState { media_id: media_id.to_owned(),
                                                       media_path: media_path.to_owned(),
                                                       fingerprint,
                                                       status: existing.status,
                                                       problems: existing.problems.clone(),
                                                       analyzed_at_ms: existing.analyzed_at_ms })?;
      tracing::info!(media_id,
                     file = %file,
                     status = ?existing.status,
                     "reused existing sidecar");
      return Ok(AnalysisOutcome { status: existing.status,
                                  detections: existing.detections.len(),
                                  sidecar: log_file_path(&sidecar_path) });
    }
  }

  tracing::info!(media_id, file = %file, step = "ffprobe", "analysis step");
  let _metadata = ffprobe_spec(&config.ffprobe, media_path).run_json().await?;
  let matcher = ProfanityMatcher::new(&config.profanity)?;
  let mut problems = Vec::new();
  let mut detections = Vec::new();
  let mut language_source_ok = false;

  tracing::info!(media_id, file = %file, step = "subtitles", "analysis step");
  for extension in ["srt", "vtt"]
  {
    let subtitle = media_path.with_extension(extension);
    if subtitle.is_file()
    {
      match tokio::fs::read_to_string(&subtitle).await
      {
        Ok(contents) => match parse_subtitles(&contents)
        {
          Ok(cues) =>
          {
            language_source_ok = true;
            for cue in cues
            {
              if !matcher.matches(&cue.text).is_empty()
              {
                detections.push(make_detection(cue.start_ms,
                                               cue.end_ms,
                                               Category::Language,
                                               Severity::Medium,
                                               1.0,
                                               "subtitle",
                                               Action::Mute,
                                               serde_json::json!({"text": cue.text, "path": subtitle})));
              }
            }
          }
          Err(error) =>
          {
            tracing::warn!(media_id,
                           file = %file,
                           subtitle = %subtitle.display(),
                           reason = %error,
                           "subtitle parse failed");
            problems.push(FilterProblem { code: "subtitle_parse_failed".into(),
                                          message: error.to_string() });
          }
        },
        Err(error) =>
        {
          tracing::warn!(media_id,
                         file = %file,
                         subtitle = %subtitle.display(),
                         reason = %error,
                         "subtitle read failed");
          problems.push(FilterProblem { code: "subtitle_read_failed".into(),
                                        message: error.to_string() });
        }
      }
    }
  }

  let whisper_configured =
    config.whisper_executable.is_some() && config.whisper_model.is_some();
  if let (Some(executable), Some(model)) = (&config.whisper_executable, &config.whisper_model)
  {
    tracing::info!(media_id, file = %file, step = "whisper", "analysis step");
    match run_whisper(config, media_path, executable, model).await
    {
      Ok(segments) =>
      {
        language_source_ok = true;
        tracing::info!(media_id,
                       file = %file,
                       segments = segments.len(),
                       "whisper transcription finished");
        for segment in segments
        {
          if !matcher.matches(&segment.text).is_empty()
          {
            detections.push(make_detection(segment.start_ms,
                                           segment.end_ms.max(segment.start_ms + 1),
                                           Category::Language,
                                           Severity::Medium,
                                           0.9,
                                           "whisper",
                                           Action::Mute,
                                           serde_json::json!({"text": segment.text})));
          }
        }
      }
      Err(error) =>
      {
        tracing::warn!(media_id, file = %file, reason = %error, "whisper failed");
        problems.push(FilterProblem { code: "whisper_failed".into(),
                                      message: error.to_string() });
      }
    }
  }

  let openvino_configured =
    config.openvino_runner.is_some() && config.openvino_model.is_some();
  if let (Some(runner), Some(model)) = (&config.openvino_runner, &config.openvino_model)
  {
    tracing::info!(media_id,
                   file = %file,
                   step = "openvino",
                   device = %config.openvino_device,
                   "analysis step");
    match run_openvino(config, media_path, runner, model).await
    {
      Ok(model_hits) =>
      {
        tracing::info!(media_id,
                       file = %file,
                       hits = model_hits.len(),
                       "openvino frame scan finished");
        for detection in model_hits
        {
          detections.push(make_detection(detection.start_ms,
                                         detection.end_ms.max(detection.start_ms + 1),
                                         detection.category,
                                         detection.severity,
                                         detection.confidence,
                                         "openvino",
                                         if matches!(detection.category,
                                                     Category::Language | Category::Profanity)
                                         {
                                           Action::Mute
                                         }
                                         else
                                         {
                                           Action::Skip
                                         },
                                         serde_json::json!({"label": detection.label})));
        }
      }
      Err(error) =>
      {
        tracing::warn!(media_id, file = %file, reason = %error, "openvino failed");
        problems.push(FilterProblem { code: "openvino_failed".into(),
                                      message: error.to_string() });
      }
    }
  }

  if !language_source_ok
  {
    problems.push(FilterProblem {
      code: "no_language_source".into(),
      message: if whisper_configured
               {
                 "Speech/subtitle language analysis did not succeed.".into()
               }
               else
               {
                 "No sidecar subtitles found and Whisper is not configured.".into()
               },
    });
  }
  if openvino_configured
     && problems.iter().any(|problem| problem.code == "openvino_failed")
     && detections.iter().all(|detection| detection.source != "openvino")
  {
    // already recorded
  }

  let status = if problems.is_empty()
               {
                 FilterReadiness::Ready
               }
               else
               {
                 FilterReadiness::Partial
               };

  let document = FilterDocument { version: FilterDocument::CURRENT_VERSION,
                                  media_id: media_id.to_owned(),
                                  media_fingerprint: fingerprint.clone(),
                                  analyzed_at_ms: chrono::Utc::now().timestamp_millis(),
                                  status,
                                  problems: problems.clone(),
                                  detections: detections.clone() };
  tracing::info!(media_id,
                 file = %file,
                 step = "sidecar",
                 sidecar = %sidecar_path.display(),
                 status = ?status,
                 detections = detections.len(),
                 problems = problems.len(),
                 "writing analysis sidecar");
  document.save(&sidecar_path)?;
  repository.put_media_state(&MediaAnalysisState { media_id: media_id.to_owned(),
                                                   media_path: media_path.to_owned(),
                                                   fingerprint,
                                                   status,
                                                   problems,
                                                   analyzed_at_ms: document.analyzed_at_ms })?;

  for detection in &detections
  {
    let evidence = Evidence { id: detection.id,
                              media_id: media_id.to_owned(),
                              start_ms: detection.start_ms,
                              end_ms: detection.end_ms,
                              category: detection.category,
                              severity: detection.severity,
                              confidence: detection.confidence,
                              source: detection.source.clone(),
                              details: detection.details.clone() };
    repository.insert_evidence(&evidence)?;
    repository.insert_candidate(&Candidate { id: Uuid::new_v4(),
                                             media_id: media_id.to_owned(),
                                             interval: Interval { start_ms: detection.start_ms,
                                                                  end_ms: detection.end_ms,
                                                                  action: detection.suggested_action,
                                                                  category: detection.category,
                                                                  severity: detection.severity },
                                             evidence_ids: vec![evidence.id],
                                             status: CandidateStatus::Pending })?;
  }
  Ok(AnalysisOutcome { status,
                       detections: detections.len(),
                       sidecar: log_file_path(&sidecar_path) })
}

#[allow(clippy::too_many_arguments)]
fn make_detection(start_ms: u64,
                  end_ms: u64,
                  category: Category,
                  severity: Severity,
                  confidence: f32,
                  source: &str,
                  suggested_action: Action,
                  details: serde_json::Value)
                  -> FilterDetection
{
  FilterDetection { id: Uuid::new_v4(),
                    start_ms,
                    end_ms,
                    category,
                    severity,
                    confidence,
                    source: source.to_owned(),
                    suggested_action,
                    details }
}

async fn run_whisper(config: &AnalysisConfig,
                     media_path: &Path,
                     executable: &Path,
                     model: &Path)
                     -> anyhow::Result<Vec<TranscriptSegment>>
{
  let temp = tempfile::tempdir()?;
  let audio = temp.path().join("audio.wav");
  ffmpeg_audio_spec(&config.ffmpeg, media_path, &audio).run().await?;
  let output_prefix = temp.path().join("transcript");
  let stdout = whisper_spec_with_options(executable,
                                         model,
                                         &audio,
                                         &output_prefix,
                                         &config.whisper_options).run()
                                                                 .await?;
  let transcript_path = output_prefix.with_extension("json");
  let bytes =
    if transcript_path.is_file() { tokio::fs::read(transcript_path).await? } else { stdout };
  whisper_segments(&bytes)
}

async fn run_openvino(config: &AnalysisConfig,
                      media_path: &Path,
                      runner: &Path,
                      model: &Path)
                      -> anyhow::Result<Vec<ModelDetection>>
{
  let temp = tempfile::tempdir()?;
  ffmpeg_frame_sample_spec(&config.ffmpeg, media_path, temp.path(), config.frame_interval_seconds).run()
                                                                                                  .await?;
  let output = openvino_spec(runner,
                             model,
                             temp.path(),
                             &config.openvino_device,
                             u64::from(config.frame_interval_seconds.max(1)) * 1000,
                             config.openvino_labels.as_deref()).run()
                                                              .await?;
  model_detections(&output)
}

#[derive(Debug, Deserialize)]
struct WhisperDocument
{
  #[serde(default)]
  transcription: Vec<WhisperEntry>,
  #[serde(default)]
  segments: Vec<WhisperEntry>,
}

#[derive(Debug, Deserialize)]
struct WhisperEntry
{
  #[serde(default)]
  text: String,
  #[serde(default)]
  start: Option<f64>,
  #[serde(default)]
  end: Option<f64>,
  #[serde(default)]
  offsets: Option<WhisperOffsets>,
}

#[derive(Debug, Deserialize)]
struct WhisperOffsets
{
  from: u64,
  to: u64,
}

struct TranscriptSegment
{
  start_ms: u64,
  end_ms: u64,
  text: String,
}

fn whisper_segments(bytes: &[u8]) -> anyhow::Result<Vec<TranscriptSegment>>
{
  let document: WhisperDocument = serde_json::from_slice(bytes)?;
  Ok(document.transcription
             .into_iter()
             .chain(document.segments)
             .map(|entry| {
               let (start_ms, end_ms) = entry.offsets
                                             .map(|value| (value.from, value.to))
                                             .unwrap_or_else(|| {
                                               ((entry.start.unwrap_or_default() * 1000.0) as u64,
                                                (entry.end.unwrap_or_default() * 1000.0) as u64)
                                             });
               TranscriptSegment { start_ms, end_ms, text: entry.text }
             })
             .collect())
}

#[derive(Debug, Deserialize)]
struct RawModelDetection
{
  start_ms: u64,
  end_ms: u64,
  category: Category,
  severity: Severity,
  confidence: f32,
  #[serde(default)]
  label: String,
}

struct ModelDetection
{
  start_ms: u64,
  end_ms: u64,
  category: Category,
  severity: Severity,
  confidence: f32,
  label: String,
}

fn model_detections(bytes: &[u8]) -> anyhow::Result<Vec<ModelDetection>>
{
  let raw: Vec<RawModelDetection> = serde_json::from_slice(bytes)?;
  Ok(raw.into_iter()
        .filter(|item| item.confidence.is_finite() && item.confidence >= 0.0)
        .map(|item| {
          ModelDetection { start_ms: item.start_ms,
                           end_ms: item.end_ms,
                           category: item.category,
                           severity: item.severity,
                           confidence: item.confidence.min(1.0),
                           label: item.label }
        })
        .collect())
}

#[cfg(test)]
mod tests
{
  use super::*;

  #[test]
  fn parses_whisper_cpp_and_segment_json()
  {
    let cpp = br#"{"transcription":[{"text":"darn","offsets":{"from":10,"to":20}}]}"#;
    let values = whisper_segments(cpp).unwrap();
    assert_eq!((values[0].start_ms, values[0].end_ms), (10, 20));

    let generic = br#"{"segments":[{"text":"x","start":1.25,"end":2.5}]}"#;
    let values = whisper_segments(generic).unwrap();
    assert_eq!((values[0].start_ms, values[0].end_ms), (1250, 2500));
  }

  #[test]
  fn filters_invalid_model_confidence()
  {
    let json =
      br#"[{"start_ms":0,"end_ms":5,"category":"violence","severity":"high","confidence":-1.0}]"#;
    assert!(model_detections(json).unwrap().is_empty());
  }
}
