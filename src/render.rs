//! Drawing 64x64 images for the LCD keys: background, icon, label.

use std::{path::Path, sync::OnceLock};

use ab_glyph::{FontVec, PxScale};
use image::{DynamicImage, Rgba, RgbaImage, imageops};
use imageproc::drawing::{draw_text_mut, text_size};

use crate::config::{Action, KeyConfig};

pub const SIZE: u32 = 64;

fn font() -> Option<&'static FontVec> {
    static FONT: OnceLock<Option<FontVec>> = OnceLock::new();
    FONT.get_or_init(|| {
        [
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\arialbd.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ]
        .iter()
        .find_map(|p| FontVec::try_from_vec(std::fs::read(p).ok()?).ok())
    })
    .as_ref()
}

pub fn parse_color(hex: &str) -> Option<[u8; 3]> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub fn render_key(key: &KeyConfig, icons_dir: &Path, mic_muted: bool) -> DynamicImage {
    let mut bg = parse_color(&key.color).unwrap_or([31, 36, 48]);
    let mut label = key.label.trim().to_string();

    if key.action == Action::MicToggle && mic_muted {
        bg = [190, 30, 45];
        label = if label.is_empty() { "OFF".into() } else { format!("{label}\nOFF") };
    }

    let mut img = RgbaImage::from_pixel(SIZE, SIZE, Rgba([bg[0], bg[1], bg[2], 255]));

    let icon = key
        .icon
        .as_ref()
        .and_then(|name| image::open(icons_dir.join(name)).ok());

    let text_top = match icon {
        Some(icon) => {
            let box_size = if label.is_empty() { 56 } else { 36 };
            let icon = icon.resize(box_size, box_size, imageops::FilterType::Lanczos3).to_rgba8();
            let x = (SIZE - icon.width()) / 2;
            let y = if label.is_empty() { (SIZE - icon.height()) / 2 } else { 3 + (box_size - icon.height()) / 2 };
            imageops::overlay(&mut img, &icon, x as i64, y as i64);
            if label.is_empty() { SIZE } else { 40 }
        }
        None => 0,
    };

    if !label.is_empty() {
        draw_label(&mut img, &label, text_top, bg);
    }

    DynamicImage::ImageRgba8(img)
}

/// Draws a centred, multi-line label in the area [top, SIZE), shrinking the font until it fits.
fn draw_label(img: &mut RgbaImage, label: &str, top: u32, bg: [u8; 3]) {
    let Some(font) = font() else { return };
    let lines: Vec<&str> = label.lines().collect();
    let avail_w = SIZE - 6;
    let avail_h = SIZE - top - 2;

    let luminance = 0.299 * bg[0] as f32 + 0.587 * bg[1] as f32 + 0.114 * bg[2] as f32;
    let color = if luminance > 160.0 { Rgba([20, 20, 20, 255]) } else { Rgba([255, 255, 255, 255]) };

    let mut px = 20.0f32;
    let line_h = |px: f32| (px * 1.05).ceil() as u32;
    while px > 8.0 {
        let scale = PxScale::from(px);
        let fits_w = lines.iter().all(|l| text_size(scale, font, l).0 <= avail_w);
        if fits_w && line_h(px) * lines.len() as u32 <= avail_h {
            break;
        }
        px -= 1.0;
    }

    let scale = PxScale::from(px);
    let total_h = line_h(px) * lines.len() as u32;
    let mut y = top as i32 + (avail_h as i32 - total_h as i32) / 2;
    for line in lines {
        let (w, _) = text_size(scale, font, line);
        let x = (SIZE as i32 - w as i32) / 2;
        draw_text_mut(img, color, x, y, scale, font, line);
        y += line_h(px) as i32;
    }
}
