use crate::db::{AnalysisJob, JobStatus, Repository};
use filter_core::{
  Action, Candidate, CandidateStatus, Category, Evidence, Interval, ProfanityConfig,
  ProfanityMatcher, Severity, ffmpeg_audio_spec, ffmpeg_frame_sample_spec, ffprobe_spec,
  media_fingerprint, openvino_spec, parse_subtitles, whisper_spec,
};
use serde::Deserialize;
use std::{
  path::{Path, PathBuf},
  sync::Arc,
};
use tokio::sync::Semaphore;
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
           openvino_device: "GPU".into(),
           frame_interval_seconds: 5,
           profanity: ProfanityConfig::default() }
  }
}

#[derive(Clone)]
pub struct JobManager
{
  repository: Repository,
  config: Arc<AnalysisConfig>,
  permits: Arc<Semaphore>,
}

impl JobManager
{
  pub fn new(repository: Repository, config: AnalysisConfig, concurrency: usize) -> Self
  {
    Self { repository,
           config: Arc::new(config),
           permits: Arc::new(Semaphore::new(concurrency.max(1))) }
  }

  pub fn enqueue(&self, media_id: &str, path: &Path) -> crate::db::Result<AnalysisJob>
  {
    let job = self.repository.create_job(media_id, path)?;
    let manager = self.clone();
    let task_job = job.clone();
    tokio::spawn(async move {
      let permit = manager.permits.acquire().await;
      let Ok(_permit) = permit
      else
      {
        return;
      };
      let _ = manager.repository.update_job(task_job.id, JobStatus::Running, None);
      match analyze(
                &manager.repository,
                &manager.config,
                &task_job.media_id,
                &task_job.media_path,
            )
            .await
            {
                Ok(()) => {
                    let _ = manager
                        .repository
                        .update_job(task_job.id, JobStatus::Complete, None);
                }
                Err(error) => {
                    tracing::warn!(job_id = %task_job.id, %error, "analysis failed");
                    let _ = manager.repository.update_job(
                        task_job.id,
                        JobStatus::Failed,
                        Some(&error.to_string()),
                    );
                }
            }
    });
    Ok(job)
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
  let _fingerprint = media_fingerprint(media_path)?;
  let _metadata = ffprobe_spec(&config.ffprobe, media_path).run_json().await?;
  let matcher = ProfanityMatcher::new(&config.profanity)?;

  for extension in ["srt", "vtt"]
  {
    let subtitle = media_path.with_extension(extension);
    if subtitle.is_file()
    {
      let contents = tokio::fs::read_to_string(&subtitle).await?;
      for cue in parse_subtitles(&contents)?
      {
        if !matcher.matches(&cue.text).is_empty()
        {
          add_detection(repository,
                        media_id,
                        cue.start_ms,
                        cue.end_ms,
                        Category::Language,
                        Severity::Medium,
                        1.0,
                        "subtitle",
                        serde_json::json!({"text": cue.text, "path": subtitle}))?;
        }
      }
    }
  }

  if let (Some(executable), Some(model)) = (&config.whisper_executable, &config.whisper_model)
  {
    let temp = tempfile::tempdir()?;
    let audio = temp.path().join("audio.wav");
    ffmpeg_audio_spec(&config.ffmpeg, media_path, &audio).run().await?;
    let output_prefix = temp.path().join("transcript");
    let stdout = whisper_spec(executable, model, &audio, &output_prefix).run().await?;
    let transcript_path = output_prefix.with_extension("json");
    let bytes =
      if transcript_path.is_file() { tokio::fs::read(transcript_path).await? } else { stdout };
    for segment in whisper_segments(&bytes)?
    {
      if !matcher.matches(&segment.text).is_empty()
      {
        add_detection(repository,
                      media_id,
                      segment.start_ms,
                      segment.end_ms.max(segment.start_ms + 1),
                      Category::Language,
                      Severity::Medium,
                      0.9,
                      "whisper",
                      serde_json::json!({"text": segment.text}))?;
      }
    }
  }

  if let (Some(runner), Some(model)) = (&config.openvino_runner, &config.openvino_model)
  {
    let temp = tempfile::tempdir()?;
    ffmpeg_frame_sample_spec(&config.ffmpeg,
                             media_path,
                             temp.path(),
                             config.frame_interval_seconds).run()
                                                           .await?;
    let output = openvino_spec(runner, model, temp.path(), &config.openvino_device).run().await?;
    for detection in model_detections(&output)?
    {
      add_detection(repository,
                    media_id,
                    detection.start_ms,
                    detection.end_ms.max(detection.start_ms + 1),
                    detection.category,
                    detection.severity,
                    detection.confidence,
                    "openvino",
                    serde_json::json!({"label": detection.label}))?;
    }
  }
  Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_detection(repository: &Repository,
                 media_id: &str,
                 start_ms: u64,
                 end_ms: u64,
                 category: Category,
                 severity: Severity,
                 confidence: f32,
                 source: &str,
                 details: serde_json::Value)
                 -> crate::db::Result<()>
{
  let evidence = Evidence { id: Uuid::new_v4(),
                            media_id: media_id.to_owned(),
                            start_ms,
                            end_ms,
                            category,
                            severity,
                            confidence,
                            source: source.to_owned(),
                            details };
  repository.insert_evidence(&evidence)?;
  repository.insert_candidate(&Candidate { id: Uuid::new_v4(),
                                           media_id: media_id.to_owned(),
                                           interval: Interval { start_ms,
                                                                end_ms,
                                                                action: if matches!(
                                                                  category,
                                                                  Category::Language
                                                                  | Category::Profanity
                                                                )
                                                                {
                                                                  Action::Mute
                                                                }
                                                                else
                                                                {
                                                                  Action::Skip
                                                                },
                                                                category,
                                                                severity },
                                           evidence_ids: vec![evidence.id],
                                           status: CandidateStatus::Pending })
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
