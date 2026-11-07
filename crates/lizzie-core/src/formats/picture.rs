//! Pictures of workbench panels (Java `saveImageToFile` / `saveImage`): the UI hands over a PNG of
//! the panel as shown; it is written in one of the chooser's formats (PNG, JPG, GIF, BMP), put on
//! the clipboard, or kept as a 存档与读档 thumbnail.
//!
//! Java writes the format of the selected chooser filter and appends that filter's extension when
//! the name does not carry it. System choosers do not report the selected filter, so here the
//! name's own extension decides and a name without one of the four gets `.png`, the first and
//! preselected filter.

use base64::Engine as _;
use image::ImageFormat;

/// The chooser filters in Java's order: `*.png`, `*.jpg` (JPG, JPEG), `*.gif`, `*.bmp`.
pub const FILTERS: &[(&str, &[&str])] = &[
    ("*.png", &["png", "PNG"]),
    ("*.jpg", &["jpg", "jpeg", "JPG", "JPEG"]),
    ("*.gif", &["gif", "GIF"]),
    ("*.bmp", &["bmp", "BMP"]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
}

/// The file a picture is written to and its format (see the module notes).
pub fn picture_target(selected: &str) -> (String, PictureFormat) {
    let lower = std::path::Path::new(selected)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let format = match lower.as_str() {
        "png" => Some(PictureFormat::Png),
        "jpg" | "jpeg" => Some(PictureFormat::Jpeg),
        "gif" => Some(PictureFormat::Gif),
        "bmp" => Some(PictureFormat::Bmp),
        _ => None,
    };
    match format {
        Some(f) => (selected.to_string(), f),
        None => (format!("{selected}.png"), PictureFormat::Png),
    }
}

/// Base64 PNG from the UI (with or without a `data:` prefix).
pub fn png_from_base64(text: &str) -> Result<Vec<u8>, String> {
    let body = text
        .split_once(',')
        .map_or(text, |(head, rest)| if head.starts_with("data:") { rest } else { text });
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|e| format!("picture is not base64: {e}"))?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("picture is not a PNG".into());
    }
    Ok(bytes)
}

/// The picture in `format` (a PNG is written as given).
pub fn encode(png: &[u8], format: PictureFormat) -> Result<Vec<u8>, String> {
    if format == PictureFormat::Png {
        return Ok(png.to_vec());
    }
    let img = image::load_from_memory_with_format(png, ImageFormat::Png).map_err(|e| e.to_string())?;
    let (img, fmt) = match format {
        // JPEG and BMP carry no transparency in Java's RGB board images either.
        PictureFormat::Jpeg => (image::DynamicImage::ImageRgb8(img.to_rgb8()), ImageFormat::Jpeg),
        PictureFormat::Bmp => (image::DynamicImage::ImageRgb8(img.to_rgb8()), ImageFormat::Bmp),
        PictureFormat::Gif => (image::DynamicImage::ImageRgba8(img.to_rgba8()), ImageFormat::Gif),
        PictureFormat::Png => unreachable!(),
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, fmt).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// RGBA pixels of a PNG, for the clipboard.
pub fn rgba(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory_with_format(png, ImageFormat::Png).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((rgba.into_raw(), w, h))
}

/// A Java slot thumbnail (`save/gameN.bmp`) as PNG.
pub fn bmp_to_png(bmp: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory_with_format(bmp, ImageFormat::Bmp).map_err(|e| e.to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_decides_the_format_and_png_is_the_default() {
        assert_eq!(picture_target("/d/a.JPEG"), ("/d/a.JPEG".into(), PictureFormat::Jpeg));
        assert_eq!(picture_target("/d/a.bmp"), ("/d/a.bmp".into(), PictureFormat::Bmp));
        assert_eq!(picture_target("/d/board"), ("/d/board.png".into(), PictureFormat::Png));
        assert_eq!(picture_target("/d/a.sgf"), ("/d/a.sgf.png".into(), PictureFormat::Png));
    }

    #[test]
    fn every_format_reads_back_with_the_same_size() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(7, 5, image::Rgba([200, 150, 80, 255]))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        for (format, fmt) in [
            (PictureFormat::Jpeg, ImageFormat::Jpeg),
            (PictureFormat::Gif, ImageFormat::Gif),
            (PictureFormat::Bmp, ImageFormat::Bmp),
        ] {
            let bytes = encode(&png, format).unwrap();
            let back = image::load_from_memory_with_format(&bytes, fmt).unwrap();
            assert_eq!((back.width(), back.height()), (7, 5), "{format:?}");
        }
    }
}
