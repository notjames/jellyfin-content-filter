use chrono::{Local, NaiveTime, Timelike};
use reqwest::Client;
use serde::Deserialize;
use std::{env, time::Duration};

#[derive(Debug, Clone)]
pub struct AnalysisSchedule
{
  window: Option<TimeWindow>,
  jellyfin: Option<JellyfinYield>,
  poll_interval: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeWindow
{
  start_minutes: u32,
  end_minutes: u32,
}

#[derive(Debug, Clone)]
struct JellyfinYield
{
  client: Client,
  base_url: String,
  api_key: String,
}

impl AnalysisSchedule
{
  pub fn always_on() -> Self
  {
    Self { window: None, jellyfin: None, poll_interval: Duration::from_secs(30) }
  }

  pub fn from_env() -> Self
  {
    let window = env::var("FILTERD_ANALYSIS_WINDOW").ok()
                                                   .and_then(|value| parse_time_window(value.as_str()).ok());
    let jellyfin = match (env::var("FILTERD_JELLYFIN_URL"), env::var("FILTERD_JELLYFIN_API_KEY"))
    {
      (Ok(url), Ok(api_key)) if !url.trim().is_empty() && !api_key.trim().is_empty() =>
      {
        Some(JellyfinYield { client: Client::new(),
                             base_url: url.trim().trim_end_matches('/').to_owned(),
                             api_key: api_key.trim().to_owned() })
      }
      _ => None,
    };
    let poll_seconds = env_parse("FILTERD_YIELD_POLL_SECONDS", 30).max(5);
    Self { window,
           jellyfin,
           poll_interval: Duration::from_secs(poll_seconds) }
  }

  pub fn poll_interval(&self) -> Duration
  {
    self.poll_interval
  }

  pub fn log_startup(&self)
  {
    match self.window
    {
      None => tracing::info!("analysis schedule: no time window (queue may run any time)"),
      Some(window) =>
      {
        tracing::info!(start_minutes = window.start_minutes,
                       end_minutes = window.end_minutes,
                       "analysis schedule: time window enabled");
      }
    }
    if self.jellyfin.is_some()
    {
      tracing::info!(poll_seconds = self.poll_interval.as_secs(),
                     "analysis schedule: yielding while Jellyfin transcodes");
    }
    else
    {
      tracing::info!("analysis schedule: Jellyfin transcode yield disabled");
    }
  }

  pub fn in_time_window(&self) -> bool
  {
    match self.window
    {
      None => true,
      Some(window) => window.contains(Local::now().time()),
    }
  }

  pub async fn can_start(&self) -> bool
  {
    if !self.in_time_window()
    {
      return false;
    }
    match &self.jellyfin
    {
      None => true,
      Some(jellyfin) => !jellyfin_transcoding_active(jellyfin).await.unwrap_or(false),
    }
  }
}

fn env_parse<T>(name: &str, default: T) -> T
  where T: std::str::FromStr
{
  std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn parse_clock(value: &str) -> Option<NaiveTime>
{
  NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()
}

fn parse_time_window(value: &str) -> Result<TimeWindow, String>
{
  let (start, end) = value.split_once('-').ok_or_else(|| "expected HH:MM-HH:MM".to_owned())?;
  let start = parse_clock(start).ok_or_else(|| format!("invalid start time: {start}"))?;
  let end = parse_clock(end).ok_or_else(|| format!("invalid end time: {end}"))?;
  Ok(TimeWindow { start_minutes: start.hour() * 60 + start.minute(),
                  end_minutes: end.hour() * 60 + end.minute() })
}

impl TimeWindow
{
  fn contains(&self, time: NaiveTime) -> bool
  {
    let minutes = time.hour() * 60 + time.minute();
    if self.start_minutes == self.end_minutes
    {
      return true;
    }
    if self.start_minutes < self.end_minutes
    {
      minutes >= self.start_minutes && minutes < self.end_minutes
    }
    else
    {
      minutes >= self.start_minutes || minutes < self.end_minutes
    }
  }
}

#[derive(Debug, Deserialize)]
struct JellyfinSession
{
  #[serde(default)]
  #[serde(rename = "TranscodingInfo")]
  transcoding_info: Option<serde_json::Value>,
}

async fn jellyfin_transcoding_active(jellyfin: &JellyfinYield) -> reqwest::Result<bool>
{
  let url = format!("{}/Sessions", jellyfin.base_url);
  let sessions: Vec<JellyfinSession> =
    jellyfin.client.get(url).header("X-Emby-Token", &jellyfin.api_key).send().await?.json().await?;
  Ok(sessions.iter().any(|session| session.transcoding_info.is_some()))
}

#[cfg(test)]
mod tests
{
  use super::*;

  #[test]
  fn parses_same_day_window()
  {
    let window = parse_time_window("09:00-17:00").unwrap();
    assert!(window.contains(NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
    assert!(!window.contains(NaiveTime::from_hms_opt(18, 0, 0).unwrap()));
  }

  #[test]
  fn parses_overnight_window()
  {
    let window = parse_time_window("22:00-06:00").unwrap();
    assert!(window.contains(NaiveTime::from_hms_opt(23, 30, 0).unwrap()));
    assert!(window.contains(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert!(!window.contains(NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
  }
}
