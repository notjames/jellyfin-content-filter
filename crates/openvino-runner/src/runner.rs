use crate::labels::LabelMap;
use crate::preprocess::{input_shape_dims, load_and_preprocess, resolve_spatial_size};
use anyhow::{Context, Result, bail};
use filter_core::{Category, Severity};
use openvino::{Core, DeviceType, ElementType, Shape, Tensor};
use serde::Serialize;
use std::{
  fs,
  path::{Path, PathBuf},
  str::FromStr,
};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize)]
pub struct Detection
{
  pub start_ms: u64,
  pub end_ms: u64,
  pub category: Category,
  pub severity: Severity,
  pub confidence: f32,
  pub label: String,
}

pub fn run(model_path: &Path,
           weights_path: Option<&Path>,
           frames_dir: &Path,
           device: &str,
           fallback_device: &str,
           frame_interval_ms: u64,
           labels: &LabelMap)
           -> Result<Vec<Detection>>
{
  let frames = list_frames(frames_dir)?;
  if frames.is_empty()
  {
    info!("no frames found under {}", frames_dir.display());
    return Ok(Vec::new());
  }

  let mut core = Core::new().context("initialize OpenVINO Core")?;
  let (model_file, weights_file) = resolve_model_paths(model_path, weights_path)?;
  let model = core.read_model_from_file(&model_file, &weights_file)
                  .with_context(|| {
                    format!("read model {} (weights {})", model_file, weights_file)
                  })?;

  let mut working_labels = labels.clone();
  if let Ok(input) = model.get_input_by_index(0)
  {
    if let Ok(shape) = input.get_shape()
    {
      let dims = shape.get_dimensions();
      let (height, width) = resolve_spatial_size(dims, labels)?;
      working_labels.input_height = height;
      working_labels.input_width = width;
    }
  }

  let mut compiled = compile_with_fallback(&mut core, &model, device, fallback_device)?;
  let mut request = compiled.create_infer_request().context("create infer request")?;
  let mut detections = Vec::new();

  for (index, frame_path) in frames.iter().enumerate()
  {
    let start_ms = index as u64 * frame_interval_ms;
    let end_ms = start_ms + frame_interval_ms;
    let pixels = load_and_preprocess(frame_path, &working_labels)
                   .with_context(|| format!("preprocess {}", frame_path.display()))?;
    let dims = input_shape_dims(&working_labels)?;
    let shape = Shape::new(&dims).context("create input shape")?;
    let mut tensor = Tensor::new(ElementType::F32, &shape).context("create input tensor")?;
    tensor.get_data_mut::<f32>()
          .context("map input tensor")?
          .copy_from_slice(&pixels);
    request.set_input_tensor(&tensor).context("set input tensor")?;
    request.infer().context("run inference")?;
    let output = request.get_output_tensor().context("read output tensor")?;
    let scores = output.get_data::<f32>().context("read output scores")?;
    let probabilities = if working_labels.softmax
                        {
                          softmax(scores)
                        }
                        else
                        {
                          scores.to_vec()
                        };
    for class in &working_labels.classes
    {
      let Some(score) = probabilities.get(class.index).copied()
      else
      {
        continue;
      };
      if !score.is_finite() || score < class.min_confidence
      {
        continue;
      }
      detections.push(Detection { start_ms,
                                  end_ms,
                                  category: class.category,
                                  severity: class.severity,
                                  confidence: score.clamp(0.0, 1.0),
                                  label: class.label.clone() });
    }
  }

  Ok(detections)
}

fn softmax(logits: &[f32]) -> Vec<f32>
{
  let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
  let exps: Vec<f32> = logits.iter().map(|value| (value - max).exp()).collect();
  let sum: f32 = exps.iter().sum();
  if !sum.is_finite() || sum <= 0.0
  {
    return vec![0.0; logits.len()];
  }
  exps.into_iter().map(|value| value / sum).collect()
}

fn compile_with_fallback<'a>(core: &mut Core,
                             model: &openvino::Model,
                             device: &'a str,
                             fallback_device: &'a str)
                             -> Result<openvino::CompiledModel>
{
  let primary = DeviceType::from_str(device).unwrap_or(DeviceType::from(device));
  match core.compile_model(model, primary)
  {
    Ok(compiled) =>
    {
      info!(device, "compiled OpenVINO model");
      Ok(compiled)
    }
    Err(error) =>
    {
      warn!(device, %error, "primary device failed; trying fallback");
      let fallback = DeviceType::from_str(fallback_device).unwrap_or(DeviceType::from(fallback_device));
      core.compile_model(model, fallback)
          .with_context(|| format!("compile model on fallback device {fallback_device}"))
    }
  }
}

fn resolve_model_paths(model_path: &Path, weights_path: Option<&Path>) -> Result<(String, String)>
{
  let model_file = model_path.to_string_lossy().into_owned();
  if let Some(weights) = weights_path
  {
    return Ok((model_file, weights.to_string_lossy().into_owned()));
  }

  let extension = model_path.extension().and_then(|value| value.to_str()).unwrap_or("");
  if extension.eq_ignore_ascii_case("xml")
  {
    let bin = model_path.with_extension("bin");
    if !bin.is_file()
    {
      bail!("IR model {} requires sibling weights file {}",
            model_path.display(),
            bin.display());
    }
    return Ok((model_file, bin.to_string_lossy().into_owned()));
  }

  // ONNX and other single-file formats: empty weights path.
  Ok((model_file, String::new()))
}

fn list_frames(frames_dir: &Path) -> Result<Vec<PathBuf>>
{
  let mut frames = Vec::new();
  for entry in fs::read_dir(frames_dir).with_context(|| format!("read {}", frames_dir.display()))?
  {
    let entry = entry?;
    let path = entry.path();
    if !path.is_file()
    {
      continue;
    }
    let Some(name) = path.file_name().and_then(|value| value.to_str())
    else
    {
      continue;
    };
    if name.starts_with("frame-") && name.ends_with(".jpg")
    {
      frames.push(path);
    }
  }
  frames.sort();
  Ok(frames)
}

#[cfg(test)]
mod tests
{
  use super::*;

  #[test]
  fn resolves_xml_to_sibling_bin()
  {
    let dir = tempfile::tempdir().unwrap();
    let xml = dir.path().join("model.xml");
    let bin = dir.path().join("model.bin");
    fs::write(&xml, b"<net/>").unwrap();
    fs::write(&bin, b"weights").unwrap();
    let (model, weights) = resolve_model_paths(&xml, None).unwrap();
    assert!(model.ends_with("model.xml"));
    assert!(weights.ends_with("model.bin"));
  }

  #[test]
  fn onnx_uses_empty_weights()
  {
    let path = PathBuf::from("/tmp/model.onnx");
    let (model, weights) = resolve_model_paths(&path, None).unwrap();
    assert!(model.ends_with("model.onnx"));
    assert!(weights.is_empty());
  }

  #[test]
  fn lists_only_frame_jpgs_sorted()
  {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("frame-00000002.jpg"), b"x").unwrap();
    fs::write(dir.path().join("frame-00000001.jpg"), b"x").unwrap();
    fs::write(dir.path().join("readme.txt"), b"x").unwrap();
    let frames = list_frames(dir.path()).unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames[0].ends_with("frame-00000001.jpg"));
    assert!(frames[1].ends_with("frame-00000002.jpg"));
  }
}
