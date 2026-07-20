use crate::labels::{LabelMap, TensorLayout};
use anyhow::{Result, bail};
use image::imageops::FilterType;
use image::DynamicImage;
use std::path::Path;

pub fn load_and_preprocess(path: &Path, labels: &LabelMap) -> Result<Vec<f32>>
{
  let image = image::open(path)?;
  preprocess_image(&image, labels)
}

fn preprocess_image(image: &DynamicImage, labels: &LabelMap) -> Result<Vec<f32>>
{
  let resized = image.resize_exact(labels.input_width, labels.input_height, FilterType::Triangle)
                     .to_rgb8();
  let (width, height) = resized.dimensions();
  let pixel_count = (width * height) as usize;
  let mut tensor = vec![0.0f32; pixel_count * 3];

  for (x, y, pixel) in resized.enumerate_pixels()
  {
    let px = x as usize;
    let py = y as usize;
    for channel in 0..3
    {
      let value = (pixel.0[channel] as f32 / labels.scale - labels.mean[channel])
                  / labels.std[channel];
      let index = match labels.layout
      {
        TensorLayout::Nchw => channel * pixel_count + py * width as usize + px,
        TensorLayout::Nhwc => (py * width as usize + px) * 3 + channel,
      };
      tensor[index] = value;
    }
  }
  Ok(tensor)
}

pub fn input_shape_dims(labels: &LabelMap) -> Result<[i64; 4]>
{
  match labels.layout
  {
    TensorLayout::Nchw => Ok([1,
                              3,
                              i64::from(labels.input_height),
                              i64::from(labels.input_width)]),
    TensorLayout::Nhwc => Ok([1,
                              i64::from(labels.input_height),
                              i64::from(labels.input_width),
                              3]),
  }
}

pub fn resolve_spatial_size(dims: &[i64], labels: &LabelMap) -> Result<(u32, u32)>
{
  let height = match labels.layout
  {
    TensorLayout::Nchw => dims.get(2).copied(),
    TensorLayout::Nhwc => dims.get(1).copied(),
  };
  let width = match labels.layout
  {
    TensorLayout::Nchw => dims.get(3).copied(),
    TensorLayout::Nhwc => dims.get(2).copied(),
  };
  let height = match height
  {
    Some(value) if value > 0 => value as u32,
    _ => labels.input_height,
  };
  let width = match width
  {
    Some(value) if value > 0 => value as u32,
    _ => labels.input_width,
  };
  if height == 0 || width == 0
  {
    bail!("unable to resolve model input height/width");
  }
  Ok((height, width))
}

#[cfg(test)]
mod tests
{
  use super::*;
  use crate::labels::LabelMap;

  #[test]
  fn builds_nchw_tensor_with_expected_len()
  {
    let labels = LabelMap::default_nsfw_binary();
    let image = DynamicImage::new_rgb8(32, 16);
    let tensor = preprocess_image(&image, &labels).unwrap();
    assert_eq!(tensor.len(), 224 * 224 * 3);
  }
}
