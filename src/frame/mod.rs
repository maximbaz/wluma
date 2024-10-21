use itertools::Itertools;

pub mod capturer;
mod object;
pub mod vulkan;

pub fn compute_perceived_lightness_percent(rgbas: &[u8], has_alpha: bool, pixels: usize) -> u8 {
    let mut v = rgbas.to_vec();
    v.truncate(10);
    println!("{:?}", v);
    let channels = if has_alpha { 4 } else { 3 };

    let (rs, gs, bs) = rgbas
        .iter()
        .take(channels * pixels)
        .chunks(channels)
        .into_iter()
        .map(|mut chunk| {
            let r = *chunk.next().unwrap();
            let g = *chunk.next().unwrap();
            let b = *chunk.next().unwrap();
            (r as f64, g as f64, b as f64)
        })
        .reduce(|(rs, gs, bs), (r, g, b)| (rs + r, gs + g, bs + b))
        .unwrap();

    let pixels = pixels as f64;
    let (r, g, b) = (rs / pixels, gs / pixels, bs / pixels);

    let result = (0.241 * r * r + 0.691 * g * g + 0.068 * b * b).sqrt() / 255.0 * 100.0;

    result.round() as u8
}
