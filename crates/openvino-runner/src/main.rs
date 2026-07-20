mod labels;
mod preprocess;
mod runner;

use anyhow::{Context, Result, bail};
use clap::Parser;
use labels::LabelMap;
use runner::run;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "openvino-runner", about = "Classify sampled frames with OpenVINO")]
struct Args
{
  /// OpenVINO IR (.xml) or ONNX model path.
  #[arg(long)]
  model: PathBuf,

  /// Optional weights (.bin) path for IR models. Defaults to the sibling of --model.
  #[arg(long)]
  weights: Option<PathBuf>,

  /// Directory containing frame-XXXXXXXX.jpg samples from ffmpeg.
  #[arg(long)]
  input: PathBuf,

  /// Preferred inference device (GPU, CPU, NPU, ...).
  #[arg(long, default_value = "GPU")]
  device: String,

  /// Device to try when --device fails.
  #[arg(long, default_value = "CPU")]
  fallback_device: String,

  /// Output format. Only json is supported.
  #[arg(long, default_value = "json")]
  output_format: String,

  /// Milliseconds between consecutive sampled frames.
  #[arg(long, default_value_t = 5000)]
  frame_interval_ms: u64,

  /// Optional JSON label map (class index → category/severity/threshold).
  #[arg(long)]
  labels: Option<PathBuf>,
}

fn main() -> Result<()>
{
  tracing_subscriber::fmt().with_writer(std::io::stderr)
                           .with_env_filter(EnvFilter::from_default_env()
                                                     .add_directive("openvino_runner=info".parse()
                                                                                         ?))
                           .init();

  let args = Args::parse();
  if args.output_format != "json"
  {
    bail!("unsupported --output-format {}; only json is supported",
          args.output_format);
  }
  if !args.input.is_dir()
  {
    bail!("--input must be a directory of sampled frames");
  }
  if args.frame_interval_ms == 0
  {
    bail!("--frame-interval-ms must be > 0");
  }

  let labels = match &args.labels
  {
    Some(path) => LabelMap::load(path).with_context(|| format!("load labels from {}", path.display()))?,
    None => LabelMap::default_nsfw_binary(),
  };

  let detections = run(&args.model,
                       args.weights.as_deref(),
                       &args.input,
                       &args.device,
                       &args.fallback_device,
                       args.frame_interval_ms,
                       &labels)?;
  serde_json::to_writer(std::io::stdout(), &detections)?;
  println!();
  Ok(())
}
