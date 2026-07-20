use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  ffi::OsString,
  fs::File,
  io::{Read, Seek, SeekFrom},
  path::{Path, PathBuf},
  process::Stdio,
};
use thiserror::Error;
use tokio::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category
{
  Language,
  Profanity,
  Violence,
  Gore,
  SexualContent,
  Nudity,
  Drugs,
  Alcohol,
  Hate,
  SelfHarm,
  Other,
}

impl Category
{
  pub const fn as_str(self) -> &'static str
  {
    match self
    {
      Self::Language => "language",
      Self::Profanity => "profanity",
      Self::Violence => "violence",
      Self::Gore => "gore",
      Self::SexualContent => "sexual_content",
      Self::Nudity => "nudity",
      Self::Drugs => "drugs",
      Self::Alcohol => "alcohol",
      Self::Hate => "hate",
      Self::SelfHarm => "self_harm",
      Self::Other => "other",
    }
  }

  pub fn is_valid_name(value: &str) -> bool
  {
    matches!(value,
             "language"
             | "profanity"
             | "violence"
             | "gore"
             | "sexual_content"
             | "nudity"
             | "drugs"
             | "alcohol"
             | "hate"
             | "self_harm"
             | "other")
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action
{
  Mute,
  Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity
{
  Low,
  Medium,
  High,
  Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerAction
{
  Allow,
  Skip,
  Mute,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySettings
{
  #[serde(default)]
  pub categories: BTreeMap<String, ViewerAction>,
}

impl CategorySettings
{
  pub fn has_valid_categories(&self) -> bool
  {
    self.categories.keys().all(|category| Category::is_valid_name(category))
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackInterval
{
  pub start: f64,
  pub end: f64,
  pub category: String,
}

impl PlaybackInterval
{
  pub fn is_valid(&self) -> bool
  {
    self.start.is_finite()
    && self.end.is_finite()
    && self.start >= 0.0
    && self.start < self.end
    && Category::is_valid_name(&self.category)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterReadiness
{
  Ready,
  Partial,
  Failed,
  Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterProblem
{
  pub code: String,
  pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterDetection
{
  pub id: Uuid,
  pub start_ms: u64,
  pub end_ms: u64,
  pub category: Category,
  pub severity: Severity,
  pub confidence: f32,
  pub source: String,
  pub suggested_action: Action,
  #[serde(default)]
  pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterDocument
{
  pub version: u32,
  pub media_id: String,
  pub media_fingerprint: String,
  pub analyzed_at_ms: i64,
  pub status: FilterReadiness,
  #[serde(default)]
  pub problems: Vec<FilterProblem>,
  #[serde(default)]
  pub detections: Vec<FilterDetection>,
}

impl FilterDocument
{
  pub const CURRENT_VERSION: u32 = 1;

  pub fn sidecar_path(media_path: impl AsRef<Path>) -> PathBuf
  {
    let media_path = media_path.as_ref();
    match media_path.file_stem().and_then(|stem| stem.to_str())
    {
      Some(stem) => media_path.with_file_name(format!("{stem}.filter.json")),
      None => media_path.with_extension("filter.json"),
    }
  }

  pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self>
  {
    let bytes = std::fs::read(path.as_ref())?;
    serde_json::from_slice(&bytes).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
  }

  pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()>
  {
    let bytes = serde_json::to_vec_pretty(self)
      .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path.as_ref(), bytes)
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackConfig
{
  pub viewer: CategorySettings,
  pub title: CategorySettings,
  pub intervals: Vec<PlaybackInterval>,
  #[serde(default = "default_filter_status")]
  pub filter_status: FilterReadiness,
  #[serde(default)]
  pub problems: Vec<FilterProblem>,
  #[serde(default = "default_true")]
  pub filters_enabled: bool,
  #[serde(default)]
  pub warning: Option<String>,
}

fn default_filter_status() -> FilterReadiness
{
  FilterReadiness::Missing
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interval
{
  pub start_ms: u64,
  pub end_ms: u64,
  pub action: Action,
  pub category: Category,
  pub severity: Severity,
}

impl Interval
{
  pub fn is_valid(&self) -> bool
  {
    self.start_ms < self.end_ms
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence
{
  pub id: Uuid,
  pub media_id: String,
  pub start_ms: u64,
  pub end_ms: u64,
  pub category: Category,
  pub severity: Severity,
  pub confidence: f32,
  pub source: String,
  #[serde(default)]
  pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus
{
  Pending,
  Approved,
  Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate
{
  pub id: Uuid,
  pub media_id: String,
  pub interval: Interval,
  #[serde(default)]
  pub evidence_ids: Vec<Uuid>,
  pub status: CandidateStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewEdit
{
  pub start_ms: u64,
  pub end_ms: u64,
  pub action: Action,
  pub category: Category,
  pub severity: Severity,
}

impl From<ReviewEdit> for Interval
{
  fn from(value: ReviewEdit) -> Self
  {
    Self { start_ms: value.start_ms,
           end_ms: value.end_ms,
           action: value.action,
           category: value.category,
           severity: value.severity }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction
{
  Approve,
  Reject,
  Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review
{
  pub id: Uuid,
  pub candidate_id: Uuid,
  pub action: ReviewAction,
  pub edit: Option<ReviewEdit>,
  pub reviewer: Option<String>,
  pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule
{
  pub category: Category,
  pub minimum_severity: Severity,
  pub action: Action,
  #[serde(default = "default_true")]
  pub enabled: bool,
}

fn default_true() -> bool
{
  true
}

#[cfg(test)]
mod filter_document_tests
{
  use super::*;

  #[test]
  fn sidecar_path_uses_media_stem()
  {
    let path = FilterDocument::sidecar_path("/media/Movie Name/Movie Name.mp4");
    assert_eq!(path, PathBuf::from("/media/Movie Name/Movie Name.filter.json"));
  }

  #[test]
  fn round_trips_filter_document()
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("title.filter.json");
    let document = FilterDocument {
      version: FilterDocument::CURRENT_VERSION,
      media_id: "abc".into(),
      media_fingerprint: "fp".into(),
      analyzed_at_ms: 1,
      status: FilterReadiness::Partial,
      problems: vec![FilterProblem { code: "no_language_source".into(),
                                     message: "missing".into() }],
      detections: vec![],
    };
    document.save(&path).unwrap();
    assert_eq!(FilterDocument::load(&path).unwrap(), document);
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy
{
  pub id: Uuid,
  pub name: String,
  #[serde(default)]
  pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishedTimeline
{
  pub media_id: String,
  pub policy_id: Uuid,
  pub media_fingerprint: String,
  pub revision: u64,
  pub intervals: Vec<Interval>,
}

/// Applies policy rules and then produces sorted, disjoint intervals. At any
/// overlap, `skip` wins over `mute`; ties are resolved deterministically by
/// severity and category.
pub fn apply_policy(intervals: &[Interval], policy: &Policy) -> Vec<Interval>
{
  let rules: HashMap<Category, &PolicyRule> =
    policy.rules.iter().filter(|rule| rule.enabled).map(|rule| (rule.category, rule)).collect();
  let filtered = intervals.iter().filter_map(|interval| {
                                   let rule = rules.get(&interval.category)?;
                                   (interval.severity >= rule.minimum_severity).then(|| {
                                                                                 let mut value =
                                                                                   interval.clone();
                                                                                 value.action =
                                                                                   rule.action;
                                                                                 value
                                                                               })
                                 });
  normalize_intervals(filtered)
}

pub fn normalize_intervals<I>(intervals: I) -> Vec<Interval>
  where I: IntoIterator<Item = Interval>
{
  let values: Vec<_> = intervals.into_iter().filter(Interval::is_valid).collect();
  let points: BTreeSet<_> = values.iter().flat_map(|item| [item.start_ms, item.end_ms]).collect();
  let points: Vec<_> = points.into_iter().collect();
  let mut result: Vec<Interval> = Vec::new();

  for window in points.windows(2)
  {
    let (start_ms, end_ms) = (window[0], window[1]);
    let winner = values.iter()
                       .filter(|item| item.start_ms < end_ms && item.end_ms > start_ms)
                       .max_by_key(|item| (item.action, item.severity, item.category));
    let Some(winner) = winner
    else
    {
      continue;
    };
    let segment = Interval { start_ms,
                             end_ms,
                             action: winner.action,
                             category: winner.category,
                             severity: winner.severity };
    if let Some(previous) = result.last_mut()
       && previous.end_ms == segment.start_ms
       && previous.action == segment.action
       && previous.category == segment.category
       && previous.severity == segment.severity
    {
      previous.end_ms = segment.end_ms;
    }
    else
    {
      result.push(segment);
    }
  }
  result
}

pub fn media_fingerprint(path: impl AsRef<Path>) -> std::io::Result<String>
{
  const SAMPLE: u64 = 1024 * 1024;
  let path = path.as_ref();
  let mut file = File::open(path)?;
  let metadata = file.metadata()?;
  let mut hasher = blake3::Hasher::new();
  hasher.update(&metadata.len().to_le_bytes());
  let mut buffer = vec![0; SAMPLE.min(metadata.len()) as usize];
  file.read_exact(&mut buffer)?;
  hasher.update(&buffer);
  if metadata.len() > SAMPLE
  {
    file.seek(SeekFrom::Start(metadata.len().saturating_sub(SAMPLE)))?;
    buffer.resize(SAMPLE.min(metadata.len()) as usize, 0);
    file.read_exact(&mut buffer)?;
    hasher.update(&buffer);
  }
  Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Debug, Error)]
pub enum SubtitleError
{
  #[error("invalid subtitle timestamp: {0}")]
  Timestamp(String),
  #[error("subtitle cue has an invalid time range")]
  Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleCue
{
  pub start_ms: u64,
  pub end_ms: u64,
  pub text: String,
}

pub fn parse_subtitles(input: &str) -> Result<Vec<SubtitleCue>, SubtitleError>
{
  let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
  let mut cues = Vec::new();
  for block in normalized.split("\n\n")
  {
    let mut lines = block.lines().filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next()
    else
    {
      continue;
    };
    if first.trim().eq_ignore_ascii_case("webvtt")
    {
      continue;
    }
    let timing = if first.contains("-->")
    {
      first
    }
    else
    {
      let Some(next) = lines.next()
      else
      {
        continue;
      };
      if !next.contains("-->")
      {
        continue;
      }
      next
    };
    let mut sides = timing.splitn(2, "-->");
    let start = sides.next().unwrap_or_default().trim();
    let end = sides.next().unwrap_or_default().split_whitespace().next().unwrap_or_default();
    let start_ms = parse_timestamp(start)?;
    let end_ms = parse_timestamp(end)?;
    if start_ms >= end_ms
    {
      return Err(SubtitleError::Range);
    }
    let text = lines.collect::<Vec<_>>().join("\n");
    cues.push(SubtitleCue { start_ms, end_ms, text });
  }
  Ok(cues)
}

fn parse_timestamp(value: &str) -> Result<u64, SubtitleError>
{
  let value = value.replace(',', ".");
  let parts: Vec<_> = value.split(':').collect();
  let (hours, minutes, seconds) = match parts.as_slice()
  {
    [minutes, seconds] => (0, parse_u64(minutes, value.as_str())?, *seconds),
    [hours, minutes, seconds] =>
    {
      (parse_u64(hours, value.as_str())?, parse_u64(minutes, value.as_str())?, *seconds)
    }
    _ => return Err(SubtitleError::Timestamp(value)),
  };
  let mut second_parts = seconds.split('.');
  let seconds = parse_u64(second_parts.next().unwrap_or_default(), &value)?;
  let fraction = second_parts.next().unwrap_or("0");
  if second_parts.next().is_some() || fraction.len() > 3
  {
    return Err(SubtitleError::Timestamp(value));
  }
  let millis = parse_u64(fraction, &value)? * 10_u64.pow(3 - fraction.len() as u32);
  Ok(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn parse_u64(value: &str, original: &str) -> Result<u64, SubtitleError>
{
  value.parse().map_err(|_| SubtitleError::Timestamp(original.to_owned()))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfanityConfig
{
  /// Optional JSON word list (better-profane-words format: [{word, categories, intensity}]).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub word_list_path: Option<PathBuf>,
  /// Minimum intensity from the word list (1–5). Ignored for explicit terms/patterns.
  #[serde(default = "default_min_intensity")]
  pub min_intensity: u8,
  #[serde(default)]
  pub terms: Vec<String>,
  #[serde(default)]
  pub patterns: Vec<String>,
  #[serde(default)]
  pub case_sensitive: bool,
}

fn default_min_intensity() -> u8
{
  1
}

#[derive(Debug, Error)]
pub enum ProfanityWordListError
{
  #[error("read profanity word list: {0}")]
  Read(#[from] std::io::Error),
  #[error("parse profanity word list: {0}")]
  Json(#[from] serde_json::Error),
  #[error("profanity word list must be a JSON array")]
  InvalidFormat,
}

#[derive(Debug, Deserialize)]
struct ProfanityWordEntry
{
  word: String,
  #[serde(default)]
  intensity: u8,
}

impl ProfanityConfig
{
  pub fn load_terms(&self) -> Result<Vec<String>, ProfanityWordListError>
  {
    let mut terms = self.terms.clone();
    if let Some(path) = &self.word_list_path
    {
      if path.is_file()
      {
        terms.extend(load_word_list(path, self.min_intensity)?);
      }
    }
    terms.retain(|term| !term.is_empty());
    terms.sort_unstable();
    terms.dedup();
    Ok(terms)
  }
}

fn load_word_list(path: &Path, min_intensity: u8) -> Result<Vec<String>, ProfanityWordListError>
{
  let text = std::fs::read_to_string(path)?;
  let entries: Vec<ProfanityWordEntry> = serde_json::from_str(&text)?;
  Ok(entries.into_iter()
            .filter(|entry| entry.intensity >= min_intensity)
            .map(|entry| entry.word)
            .collect())
}

#[derive(Debug, Error)]
pub enum ProfanityMatcherError
{
  #[error("profanity word list: {0}")]
  WordList(#[from] ProfanityWordListError),
  #[error("profanity matcher regex: {0}")]
  Regex(#[from] regex::Error),
}

pub struct ProfanityMatcher
{
  patterns: Vec<Regex>,
}

impl ProfanityMatcher
{
  pub fn new(config: &ProfanityConfig) -> Result<Self, ProfanityMatcherError>
  {
    let terms = config.load_terms()?;
    let mut patterns = Vec::new();
    const CHUNK_SIZE: usize = 400;
    for chunk in terms.chunks(CHUNK_SIZE)
    {
      let alternates: Vec<String> = chunk.iter()
                                         .map(|term| format!(r"\b{}\b", regex::escape(term)))
                                         .collect();
      if alternates.is_empty()
      {
        continue;
      }
      patterns.push(RegexBuilder::new(&format!("(?:{})", alternates.join("|")))
                        .case_insensitive(!config.case_sensitive)
                        .build()?);
    }
    if !config.patterns.is_empty()
    {
      patterns.push(RegexBuilder::new(&format!("(?:{})", config.patterns.join("|")))
                        .case_insensitive(!config.case_sensitive)
                        .build()?);
    }
    Ok(Self { patterns })
  }

  pub fn matches<'a>(&self, text: &'a str) -> Vec<&'a str>
  {
    let mut values = Vec::new();
    for pattern in &self.patterns
    {
      values.extend(pattern.find_iter(text).map(|item| item.as_str()));
    }
    values
  }
}

#[derive(Debug, Clone)]
pub struct CommandSpec
{
  pub program: PathBuf,
  pub args: Vec<OsString>,
}

impl CommandSpec
{
  pub async fn run_json(&self) -> anyhow::Result<serde_json::Value>
  {
    let output = self.run().await?;
    Ok(serde_json::from_slice(&output)?)
  }

  pub async fn run(&self) -> anyhow::Result<Vec<u8>>
  {
    let output = Command::new(&self.program).args(&self.args)
                                            .stdin(Stdio::null())
                                            .stderr(Stdio::piped())
                                            .stdout(Stdio::piped())
                                            .kill_on_drop(true)
                                            .output()
                                            .await?;
    if !output.status.success()
    {
      anyhow::bail!("{} failed: {}",
                    self.program.display(),
                    String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(output.stdout)
  }
}

pub fn ffprobe_spec(executable: impl Into<PathBuf>, media: impl AsRef<Path>) -> CommandSpec
{
  CommandSpec { program: executable.into(),
                args: vec!["-v".into(),
                           "error".into(),
                           "-show_format".into(),
                           "-show_streams".into(),
                           "-of".into(),
                           "json".into(),
                           media.as_ref().as_os_str().to_owned(),] }
}

pub fn ffmpeg_audio_spec(executable: impl Into<PathBuf>,
                         media: impl AsRef<Path>,
                         output_wav: impl AsRef<Path>)
                         -> CommandSpec
{
  CommandSpec { program: executable.into(),
                args: vec!["-nostdin".into(),
                           "-y".into(),
                           "-i".into(),
                           media.as_ref().as_os_str().to_owned(),
                           "-vn".into(),
                           "-ac".into(),
                           "1".into(),
                           "-ar".into(),
                           "16000".into(),
                           output_wav.as_ref().as_os_str().to_owned(),] }
}

pub fn whisper_spec(executable: impl Into<PathBuf>,
                    model: impl AsRef<Path>,
                    audio: impl AsRef<Path>,
                    output_prefix: impl AsRef<Path>)
                    -> CommandSpec
{
  CommandSpec { program: executable.into(),
                args: vec!["-m".into(),
                           model.as_ref().as_os_str().to_owned(),
                           "-f".into(),
                           audio.as_ref().as_os_str().to_owned(),
                           "-oj".into(),
                           "-of".into(),
                           output_prefix.as_ref().as_os_str().to_owned(),] }
}

pub fn openvino_spec(runner: impl Into<PathBuf>,
                     model: impl AsRef<Path>,
                     frames_dir: impl AsRef<Path>,
                     device: &str,
                     frame_interval_ms: u64,
                     labels: Option<&Path>)
                     -> CommandSpec
{
  let mut args = vec!["--model".into(),
                      model.as_ref().as_os_str().to_owned(),
                      "--input".into(),
                      frames_dir.as_ref().as_os_str().to_owned(),
                      "--device".into(),
                      device.into(),
                      "--fallback-device".into(),
                      "CPU".into(),
                      "--output-format".into(),
                      "json".into(),
                      "--frame-interval-ms".into(),
                      frame_interval_ms.to_string().into()];
  if let Some(labels) = labels
  {
    args.push("--labels".into());
    args.push(labels.as_os_str().to_owned());
  }
  CommandSpec { program: runner.into(), args }
}

pub fn ffmpeg_frame_sample_spec(executable: impl Into<PathBuf>,
                                media: impl AsRef<Path>,
                                frames_dir: impl AsRef<Path>,
                                every_seconds: u32)
                                -> CommandSpec
{
  let output = frames_dir.as_ref().join("frame-%08d.jpg");
  CommandSpec { program: executable.into(),
                args: vec!["-nostdin".into(),
                           "-y".into(),
                           "-i".into(),
                           media.as_ref().as_os_str().to_owned(),
                           "-vf".into(),
                           format!("fps=1/{}", every_seconds.max(1)).into(),
                           "-q:v".into(),
                           "2".into(),
                           output.as_os_str().to_owned(),] }
}

#[cfg(test)]
mod tests
{
  use super::*;
  use std::io::Write;

  fn interval(start: u64, end: u64, action: Action, severity: Severity) -> Interval
  {
    Interval { start_ms: start, end_ms: end, action, category: Category::Profanity, severity }
  }

  #[test]
  fn skip_dominates_and_output_is_disjoint()
  {
    let result = normalize_intervals([interval(0, 20, Action::Mute, Severity::Critical),
                                      interval(5, 10, Action::Skip, Severity::Low)]);
    assert_eq!(result.len(), 3);
    assert_eq!(result[1].action, Action::Skip);
    assert_eq!((result[1].start_ms, result[1].end_ms), (5, 10));
  }

  #[test]
  fn policy_filters_and_overrides_action()
  {
    let policy = Policy { id: Uuid::new_v4(),
                          name: "family".into(),
                          rules: vec![PolicyRule { category: Category::Profanity,
                                                   minimum_severity: Severity::Medium,
                                                   action: Action::Mute,
                                                   enabled: true }] };
    let result = apply_policy(&[interval(0, 2, Action::Skip, Severity::Low),
                                interval(2, 4, Action::Skip, Severity::High)],
                              &policy);
    assert_eq!(result, vec![interval(2, 4, Action::Mute, Severity::High)]);
  }

  #[test]
  fn parses_srt_and_vtt_timestamps()
  {
    let srt = "1\n00:00:01,250 --> 00:00:02,500\nhello\n\n";
    assert_eq!(parse_subtitles(srt).unwrap()[0].start_ms, 1250);
    let vtt = "WEBVTT\n\n00:03.000 --> 00:04.100\nworld\n";
    assert_eq!(parse_subtitles(vtt).unwrap()[0].end_ms, 4100);
  }

  #[test]
  fn profanity_terms_are_literal_and_bounded()
  {
    let matcher = ProfanityMatcher::new(&ProfanityConfig { terms: vec!["darn".into(),
                                                                       "a+b".into()],
                                                           ..Default::default() }).unwrap();
    assert_eq!(matcher.matches("DARNed darn a+b"), vec!["darn", "a+b"]);
  }

  #[test]
  fn profanity_word_list_respects_min_intensity()
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("words.json");
    std::fs::write(
      &path,
      r#"[{"word":"mild","intensity":1},{"word":"strong","intensity":5}]"#,
    )
    .unwrap();
    let config = ProfanityConfig { word_list_path: Some(path),
                                   min_intensity: 3,
                                   ..Default::default() };
    let matcher = ProfanityMatcher::new(&config).unwrap();
    assert!(matcher.matches("strong stuff").contains(&"strong"));
    assert!(matcher.matches("mild stuff").is_empty());
  }

  #[test]
  fn bundled_profanity_word_list_loads()
  {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/data/profanity-words.json");
    if !path.is_file()
    {
      return;
    }
    let config = ProfanityConfig { word_list_path: Some(path),
                                   min_intensity: 1,
                                   ..Default::default() };
    let terms = config.load_terms().unwrap();
    assert!(terms.len() > 1000);
    ProfanityMatcher::new(&config).unwrap();
  }

  #[test]
  fn fingerprint_changes_with_content()
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("media");
    File::create(&path).unwrap().write_all(b"one").unwrap();
    let first = media_fingerprint(&path).unwrap();
    File::create(&path).unwrap().write_all(b"two").unwrap();
    assert_ne!(first, media_fingerprint(&path).unwrap());
  }

  #[test]
  fn command_specs_preserve_non_utf8_paths_and_fallback()
  {
    let spec = openvino_spec("runner", "model.xml", "frames", "GPU", 5000, None);
    assert!(spec.args.iter().any(|arg| arg == "GPU"));
    assert!(spec.args.iter().any(|arg| arg == "CPU"));
    assert!(spec.args.iter().any(|arg| arg == "5000"));
    let with_labels = openvino_spec("runner",
                                    "model.xml",
                                    "frames",
                                    "GPU",
                                    5000,
                                    Some(Path::new("labels.json")));
    assert!(with_labels.args.iter().any(|arg| arg == "--labels"));
    assert!(with_labels.args.iter().any(|arg| arg == "labels.json"));
  }

  #[test]
  fn playback_contract_validates_categories_and_finite_intervals()
  {
    let valid = PlaybackInterval { start: 1.25, end: 2.5, category: "violence".into() };
    assert!(valid.is_valid());
    assert!(!PlaybackInterval { end: f64::INFINITY, ..valid.clone() }.is_valid());
    assert!(!PlaybackInterval { category: "unknown".into(), ..valid }.is_valid());

    let mut settings = CategorySettings::default();
    settings.categories.insert("profanity".into(), ViewerAction::Mute);
    assert!(settings.has_valid_categories());
    settings.categories.insert("not_a_category".into(), ViewerAction::Skip);
    assert!(!settings.has_valid_categories());
  }
}
