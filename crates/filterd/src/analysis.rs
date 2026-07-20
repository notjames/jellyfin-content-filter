use crate::db::{AnalysisJob, JobStatus, MediaAnalysisState, Repository};
use filter_core::{
  Action, Candidate, CandidateStatus, Category, Evidence, FilterDetection, FilterDocument,
  FilterProblem, FilterReadiness, Interval, ProfanityConfig, ProfanityMatcher, Severity,
  ffmpeg_audio_spec, ffmpeg_frame_sample_spec, ffprobe_spec, media_fingerprint, openvino_spec,
  parse_subtitles, whisper_spec,
};
use serde::Deserialize;
use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AnalysisConfig
{
  pub ffprobe: PathBuf,
  pub ffmpeg: PathBuf,
  pub whisper_executable: Option<PathBuf>,
  pub whisper_model: Option<PathBuf>,
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
  permits: Arc<Semaphore>,
  notify: Arc<Notify>,
}

impl JobManager
{
  pub fn new(repository: Repository, config: AnalysisConfig, concurrency: usize) -> Self
  {
    Self { repository,
           config: Arc::new(config),
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
        Ok(stats) => tracing::info!(requeued_running = stats.requeued_running,
                                    collapsed_duplicates = stats.collapsed_duplicates,
                                    "recovered analysis queue"),
        Err(error) => tracing::error!(%error, "failed to recover analysis queue"),
      }
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
    self.notify.notify_one();
    Ok(job)
  }

  async fn dispatch_loop(&self)
  {
    loop
    {
      while matches!(self.repository.has_queued_jobs(), Ok(false))
      {
        self.notify.notified().await;
      }

      let permit = match self.permits.clone().acquire_owned().await
      {
        Ok(permit) => permit,
        Err(_) => return,
      };

      let job = match self.repository.claim_next_queued()
      {
        Ok(Some(job)) => job,
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
    match analyze(&self.repository, &self.config, &job.media_id, &job.media_path).await
    {
      Ok(()) =>
      {
        let _ = self.repository.update_job(job.id, JobStatus::Complete, None);
      }
      Err(error) =>
      {
        tracing::warn!(job_id = %job.id, %error, "analysis failed");
        let message = error.to_string();
        let _ = self.repository.put_media_state(&MediaAnalysisState {
          media_id: job.media_id.clone(),
          media_path: job.media_path.clone(),
          fingerprint: String::new(),
          status: FilterReadiness::Failed,
          problems: vec![FilterProblem { code: "analysis_failed".into(),
                                         message: message.clone() }],
          analyzed_at_ms: chrono::Utc::now().timestamp_millis(),
        });
        let sidecar = FilterDocument::sidecar_path(&job.media_path);
        let failed = FilterDocument { version: FilterDocument::CURRENT_VERSION,
                                      media_id: job.media_id.clone(),
                                      media_fingerprint: String::new(),
                                      analyzed_at_ms: chrono::Utc::now().timestamp_millis(),
                                      status: FilterReadiness::Failed,
                                      problems: vec![FilterProblem { code: "analysis_failed".into(),
                                                                     message: message.clone() }],
                                      detections: vec![] };
        let _ = failed.save(&sidecar);
        let _ = self.repository.update_job(job.id, JobStatus::Failed, Some(&message));
      }
    }
  }
}

async fn analyze(repository: &Repository,
                 config: &AnalysisConfig,
                 media_id: &str,
                 media_path: &Path)
                 -> anyhow::Result<()>
{
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
      return Ok(());
    }
  }

  let _metadata = ffprobe_spec(&config.ffprobe, media_path).run_json().await?;
  let matcher = ProfanityMatcher::new(&config.profanity)?;
  let mut problems = Vec::new();
  let mut detections = Vec::new();
  let mut language_source_ok = false;

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
          Err(error) => problems.push(FilterProblem { code: "subtitle_parse_failed".into(),
                                                      message: error.to_string() }),
        },
        Err(error) => problems.push(FilterProblem { code: "subtitle_read_failed".into(),
                                                    message: error.to_string() }),
      }
    }
  }

  let whisper_configured =
    config.whisper_executable.is_some() && config.whisper_model.is_some();
  if let (Some(executable), Some(model)) = (&config.whisper_executable, &config.whisper_model)
  {
    match run_whisper(config, media_path, executable, model).await
    {
      Ok(segments) =>
      {
        language_source_ok = true;
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
      Err(error) => problems.push(FilterProblem { code: "whisper_failed".into(),
                                                  message: error.to_string() }),
    }
  }

  let openvino_configured =
    config.openvino_runner.is_some() && config.openvino_model.is_some();
  if let (Some(runner), Some(model)) = (&config.openvino_runner, &config.openvino_model)
  {
    match run_openvino(config, media_path, runner, model).await
    {
      Ok(model_hits) =>
      {
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
      Err(error) => problems.push(FilterProblem { code: "openvino_failed".into(),
                                                  message: error.to_string() }),
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
  Ok(())
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
  let stdout = whisper_spec(executable, model, &audio, &output_prefix).run().await?;
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
