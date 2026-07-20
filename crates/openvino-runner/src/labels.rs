use anyhow::{Context, Result, bail};
use filter_core::{Category, Severity};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TensorLayout
{
  Nchw,
  Nhwc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassMapping
{
  pub index: usize,
  pub label: String,
  pub category: Category,
  pub severity: Severity,
  #[serde(default = "default_min_confidence")]
  pub min_confidence: f32,
}

fn default_min_confidence() -> f32
{
  0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelMap
{
  #[serde(default = "default_layout")]
  pub layout: TensorLayout,
  #[serde(default = "default_size")]
  pub input_height: u32,
  #[serde(default = "default_size")]
  pub input_width: u32,
  /// Divide raw pixel values by this before mean/std (typically 255.0).
  #[serde(default = "default_scale")]
  pub scale: f32,
  #[serde(default = "default_mean")]
  pub mean: [f32; 3],
  #[serde(default = "default_std")]
  pub std: [f32; 3],
  /// Apply softmax to the output vector before thresholding (needed for ViT logits).
  #[serde(default)]
  pub softmax: bool,
  pub classes: Vec<ClassMapping>,
}

fn default_layout() -> TensorLayout
{
  TensorLayout::Nchw
}

fn default_size() -> u32
{
  224
}

fn default_scale() -> f32
{
  255.0
}

fn default_mean() -> [f32; 3]
{
  [0.485, 0.456, 0.406]
}

fn default_std() -> [f32; 3]
{
  [0.229, 0.224, 0.225]
}

impl LabelMap
{
  pub fn load(path: &Path) -> Result<Self>
  {
    let text = fs::read_to_string(path)?;
    let map: Self = serde_json::from_str(&text).context("parse label map JSON")?;
    map.validate()?;
    Ok(map)
  }

  /// Default profile for a common 2-class NSFW classifier (index 1 = NSFW → nudity).
  pub fn default_nsfw_binary() -> Self
  {
    Self { layout: TensorLayout::Nchw,
           input_height: 224,
           input_width: 224,
           scale: 255.0,
           mean: default_mean(),
           std: default_std(),
           softmax: false,
           classes: vec![ClassMapping { index: 1,
                                        label: "nsfw".into(),
                                        category: Category::Nudity,
                                        severity: Severity::High,
                                        min_confidence: 0.5 }] }
  }

  fn validate(&self) -> Result<()>
  {
    if self.input_height == 0 || self.input_width == 0
    {
      bail!("label map input_height/input_width must be > 0");
    }
    if !self.scale.is_finite() || self.scale <= 0.0
    {
      bail!("label map scale must be a positive finite number");
    }
    if self.classes.is_empty()
    {
      bail!("label map must define at least one class mapping");
    }
    for class in &self.classes
    {
      if !class.min_confidence.is_finite() || !(0.0..=1.0).contains(&class.min_confidence)
      {
        bail!("class {} min_confidence must be in [0, 1]", class.label);
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests
{
  use super::*;
  use std::io::Write;

  #[test]
  fn loads_example_label_map()
  {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/labels-nsfw-binary.json");
    let map = LabelMap::load(&path).unwrap();
    assert_eq!(map.classes.len(), 1);
    assert_eq!(map.classes[0].category, Category::Nudity);
  }

  #[test]
  fn rejects_empty_classes()
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    let mut file = fs::File::create(&path).unwrap();
    write!(file, r#"{{"classes":[]}}"#).unwrap();
    assert!(LabelMap::load(&path).is_err());
  }
}
