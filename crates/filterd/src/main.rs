use anyhow::Context;
use filter_core::ProfanityConfig;
use filterd::{AppState, analysis::AnalysisConfig, app, db::Repository};
use std::{
  env,
  os::unix::fs::{FileTypeExt, PermissionsExt},
  path::PathBuf,
};
use tokio::net::UnixListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()>
{
  tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("filterd=info")),
        )
        .init();

  let socket_path = env_path("FILTERD_SOCKET", "/tmp/filterd.sock");
  let database_path = env_path("FILTERD_DATABASE", "/var/lib/filterd/filterd.sqlite3");
  if let Some(parent) = database_path.parent()
  {
    std::fs::create_dir_all(parent).with_context(|| {
                                     format!("create database directory {}", parent.display())
                                   })?;
  }
  if let Some(parent) = socket_path.parent()
  {
    std::fs::create_dir_all(parent).with_context(|| {
                                     format!("create socket directory {}", parent.display())
                                   })?;
  }
  remove_stale_socket(&socket_path)?;

  let repository = Repository::open(&database_path).with_context(|| {
                                                     format!("open database {}",
                                                             database_path.display())
                                                   })?;
  let analysis =
    AnalysisConfig { ffprobe: env_path("FILTERD_FFPROBE", "ffprobe"),
                     ffmpeg: env_path("FILTERD_FFMPEG", "ffmpeg"),
                     whisper_executable:
                       env::var_os("FILTERD_WHISPER_EXECUTABLE").map(PathBuf::from),
                     whisper_model: env::var_os("FILTERD_WHISPER_MODEL").map(PathBuf::from),
                     openvino_runner: env::var_os("FILTERD_OPENVINO_RUNNER").map(PathBuf::from),
                     openvino_model: env::var_os("FILTERD_OPENVINO_MODEL").map(PathBuf::from),
                     openvino_device: env::var("FILTERD_OPENVINO_DEVICE").unwrap_or_else(|_| {
                                                                           "GPU".into()
                                                                         }),
                     frame_interval_seconds: env_parse("FILTERD_FRAME_INTERVAL_SECONDS", 5),
                     profanity: ProfanityConfig { terms: env_list("FILTERD_PROFANITY_TERMS"),
                                                  patterns:
                                                    env_list("FILTERD_PROFANITY_PATTERNS"),
                                                  case_sensitive:
                                                    env_bool("FILTERD_PROFANITY_CASE_SENSITIVE",
                                                             false) } };
  let state = AppState::new(repository,
                            analysis,
                            env_parse("FILTERD_ANALYSIS_CONCURRENCY", 2),
                            env::var("FILTERD_AUTH_TOKEN").ok().filter(|value| !value.is_empty()));
  let listener =
    UnixListener::bind(&socket_path).with_context(|| format!("bind {}", socket_path.display()))?;
  std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o660))?;
  tracing::info!(socket = %socket_path.display(), "filterd listening");
  axum::serve(listener, app(state, env_parse("FILTERD_MAX_REQUEST_BYTES", 1024 * 1024))).await?;
  Ok(())
}

fn env_path(name: &str, default: &str) -> PathBuf
{
  env::var_os(name).map(PathBuf::from).unwrap_or_else(|| default.into())
}

fn env_list(name: &str) -> Vec<String>
{
  env::var(name).ok()
                .map(|value| {
                  value.split(',')
                       .map(str::trim)
                       .filter(|value| !value.is_empty())
                       .map(str::to_owned)
                       .collect()
                })
                .unwrap_or_default()
}

fn env_parse<T>(name: &str, default: T) -> T
  where T: std::str::FromStr
{
  env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool
{
  env::var(name).ok()
                .and_then(|value| {
                  match value.to_ascii_lowercase().as_str()
                  {
                    "1" | "true" | "yes" => Some(true),
                    "0" | "false" | "no" => Some(false),
                    _ => None,
                  }
                })
                .unwrap_or(default)
}

fn remove_stale_socket(path: &std::path::Path) -> anyhow::Result<()>
{
  match std::fs::symlink_metadata(path)
  {
    Ok(metadata) if metadata.file_type().is_socket() =>
    {
      std::fs::remove_file(path)?;
      Ok(())
    }
    Ok(_) => anyhow::bail!("refusing to replace non-socket {}", path.display()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}
