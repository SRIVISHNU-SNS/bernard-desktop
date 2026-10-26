use crate::models::{Attachment, SNIP_MAX_SIDE};
use base64::Engine;
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, RgbaImage};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Manager};

use crate::paths;

#[derive(Debug, Clone)]
pub struct MonitorCapture {
    pub label: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub logical_width: f64,
    pub logical_height: f64,
    pub image: RgbaImage,
    pub preview_data_url: String,
}

#[derive(Debug, Clone)]
pub struct CaptureBatch {
    pub monitors: Vec<MonitorCapture>,
    pub started_at: Instant,
    pub elapsed_ms: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPayload {
    pub label: String,
    pub name: String,
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub image_data_url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn capture_all() -> Result<CaptureBatch, String> {
    let started = Instant::now();
    let monitors = xcap::Monitor::all()
        .map_err(|e| format!("Windows would not enumerate the displays: {e}"))?;
    if monitors.is_empty() {
        return Err("No displays were found.".to_string());
    }

    let mut captures = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.into_iter().enumerate() {
        let name = monitor
            .friendly_name()
            .or_else(|_| monitor.name())
            .unwrap_or_else(|_| format!("Display {}", index + 1));
        let x = monitor
            .x()
            .map_err(|e| format!("Could not read display position: {e}"))?;
        let y = monitor
            .y()
            .map_err(|e| format!("Could not read display position: {e}"))?;
        let width = monitor
            .width()
            .map_err(|e| format!("Could not read display size: {e}"))?;
        let height = monitor
            .height()
            .map_err(|e| format!("Could not read display size: {e}"))?;
        let scale_factor = monitor.scale_factor().unwrap_or(1.0) as f64;
        let image = monitor
            .capture_image()
            .map_err(|e| format!("Could not capture display {name}: {e}"))?;

        let logical_width = if scale_factor > 0.0 {
            width as f64 / scale_factor
        } else {
            width as f64
        };
        let logical_height = if scale_factor > 0.0 {
            height as f64 / scale_factor
        } else {
            height as f64
        };
        let preview = jpeg_data_url(&image, 82)?;

        captures.push(MonitorCapture {
            label: format!("overlay-{index}"),
            name,
            x,
            y,
            width,
            height,
            scale_factor,
            logical_width,
            logical_height,
            image,
            preview_data_url: preview,
        });
    }

    Ok(CaptureBatch {
        monitors: captures,
        started_at: started,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

fn jpeg_data_url(image: &RgbaImage, quality: u8) -> Result<String, String> {
    let mut bytes = Vec::with_capacity((image.width() * image.height() / 5) as usize);
    let mut cursor = Cursor::new(&mut bytes);
    JpegEncoder::new_with_quality(&mut cursor, quality)
        .encode_image(image)
        .map_err(|e| format!("Could not prepare the frozen display image: {e}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn payload_for(monitor: &MonitorCapture) -> OverlayPayload {
    OverlayPayload {
        label: monitor.label.clone(),
        name: monitor.name.clone(),
        monitor_x: monitor.x,
        monitor_y: monitor.y,
        width: monitor.width,
        height: monitor.height,
        scale_factor: monitor.scale_factor,
        viewport_width: monitor.logical_width,
        viewport_height: monitor.logical_height,
        image_data_url: monitor.preview_data_url.clone(),
    }
}

pub fn crop_selection(
    app: &AppHandle,
    monitor: &MonitorCapture,
    selection: &Selection,
) -> Result<Attachment, String> {
    if !selection.width.is_finite()
        || !selection.height.is_finite()
        || !selection.x.is_finite()
        || !selection.y.is_finite()
    {
        return Err("The snip selection was not valid.".to_string());
    }
    if selection.width < 4.0 || selection.height < 4.0 {
        return Err("Drag a larger area to snip.".to_string());
    }

    let scale_x = if monitor.logical_width > 0.0 {
        monitor.image.width() as f64 / monitor.logical_width
    } else {
        monitor.scale_factor
    };
    let scale_y = if monitor.logical_height > 0.0 {
        monitor.image.height() as f64 / monitor.logical_height
    } else {
        monitor.scale_factor
    };

    let left = (selection.x * scale_x).round().max(0.0) as u32;
    let top = (selection.y * scale_y).round().max(0.0) as u32;
    let right = ((selection.x + selection.width) * scale_x)
        .round()
        .min(monitor.image.width() as f64) as u32;
    let bottom = ((selection.y + selection.height) * scale_y)
        .round()
        .min(monitor.image.height() as f64) as u32;
    if right <= left || bottom <= top {
        return Err("The snip selection fell outside the display.".to_string());
    }

    let crop_width = right - left;
    let crop_height = bottom - top;
    let cropped = image::imageops::crop_imm(
        &monitor.image,
        left,
        top,
        crop_width,
        crop_height,
    )
    .to_image();

    let text_heavy = is_text_heavy(&cropped);
    let max_side = if text_heavy {
        SNIP_MAX_SIDE
    } else {
        (SNIP_MAX_SIDE * 3) / 4
    };
    let resized = downscale_to(cropped, max_side);
    save_attachment(app, resized, "snip")
}

fn is_text_heavy(image: &RgbaImage) -> bool {
    if image.width() < 16 || image.height() < 16 {
        return false;
    }
    let step = (image.width().max(image.height()) / 240).max(1);
    let mut previous_row: Option<Vec<u8>> = None;
    let mut edges = 0u64;
    let mut comparisons = 0u64;

    for y in (0..image.height()).step_by(step as usize) {
        let mut row = Vec::with_capacity((image.width() / step) as usize);
        for x in (0..image.width()).step_by(step as usize) {
            let pixel = image.get_pixel(x, y);
            let alpha = pixel[3] as u32;
            let luma = if alpha == 0 {
                255u8
            } else {
                ((pixel[0] as u32 * 299 + pixel[1] as u32 * 587 + pixel[2] as u32 * 114)
                    / 1000) as u8
            };
            if let Some(previous) = row.last() {
                comparisons += 1;
                if luma.abs_diff(*previous) > 24 {
                    edges += 1;
                }
            }
            if let Some(previous) = previous_row.as_ref().and_then(|r| r.get(row.len())) {
                comparisons += 1;
                if luma.abs_diff(*previous) > 24 {
                    edges += 1;
                }
            }
            row.push(luma);
        }
        previous_row = Some(row);
    }

    comparisons > 0 && edges as f64 / comparisons as f64 > 0.075
}

fn downscale_to(image: RgbaImage, max_side: u32) -> RgbaImage {
    let longest = image.width().max(image.height());
    if longest <= max_side {
        return image;
    }
    let scale = max_side as f64 / longest as f64;
    let width = ((image.width() as f64 * scale).round() as u32).max(1);
    let height = ((image.height() as f64 * scale).round() as u32).max(1);
    image::imageops::resize(&image, width, height, FilterType::Lanczos3)
}

fn save_attachment(
    app: &AppHandle,
    image: RgbaImage,
    prefix: &str,
) -> Result<Attachment, String> {
    let dir = paths::captures_dir(app)?;
    let path = dir.join(format!("{prefix}-{}.png", uuid::Uuid::new_v4()));
    let dynamic = DynamicImage::ImageRgba8(image);
    dynamic
        .save_with_format(&path, image::ImageFormat::Png)
        .map_err(|e| format!("Could not save the image: {e}"))?;
    attachment_from_path(&path)
}

pub fn attachment_from_path(path: &Path) -> Result<Attachment, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Could not read the attached image: {e}"))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("The attached file is not a readable image: {e}"))?;
    let width = image.width();
    let height = image.height();
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    Ok(Attachment {
        path: path.to_string_lossy().to_string(),
        data_url,
        width,
        height,
    })
}

pub fn save_data_url(app: &AppHandle, data_url: &str, prefix: &str) -> Result<Attachment, String> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, payload)| payload)
        .ok_or_else(|| "The pasted image data was not valid.".to_string())?;
    if encoded.len() > 64 * 1024 * 1024 {
        return Err("That image is larger than 64 MB.".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("Could not decode the pasted image: {e}"))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("The pasted data is not a supported image: {e}"))?
        .to_rgba8();
    let resized = downscale_to(image, SNIP_MAX_SIDE);
    save_attachment(app, resized, prefix)
}

pub fn import_clipboard(app: &AppHandle) -> Result<Attachment, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Could not open the clipboard: {e}"))?;
    let image = clipboard
        .get_image()
        .map_err(|_| "There is no image on the clipboard.".to_string())?;
    let width = image.width as u32;
    let height = image.height as u32;
    let buffer = RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or_else(|| "The clipboard image was malformed.".to_string())?;
    let resized = downscale_to(buffer, SNIP_MAX_SIDE);
    save_attachment(app, resized, "paste")
}

pub fn copy_text(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Could not open the clipboard: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Could not copy to the clipboard: {e}"))
}

pub fn remove_capture_files(paths: &[String]) {
    for path in paths {
        let candidate = PathBuf::from(path);
        if candidate
            .components()
            .any(|part| part.as_os_str() == "captures")
        {
            let _ = std::fs::remove_file(candidate);
        }
    }
}

pub fn save_pending_image(
    app: &AppHandle,
    path: &Path,
) -> Result<Attachment, String> {
    let data = std::fs::read(path).map_err(|e| format!("Could not read the dropped image: {e}"))?;
    let image = image::load_from_memory(&data)
        .map_err(|e| format!("The dropped file is not a readable image: {e}"))?
        .to_rgba8();
    save_attachment(app, downscale_to(image, SNIP_MAX_SIDE), "drop")
}
