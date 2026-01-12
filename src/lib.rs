use image::imageops::FilterType;
use lopdf::{Document, Object};
use pixo::jpeg::JpegOptions;
use pixo::png::{PngOptions, QuantizationMode};
use pixo::{ColorType, jpeg, png};
use wasm_bindgen::prelude::*;

#[inline]
fn clamp_u16(v: u16, lo: u16, hi: u16) -> u16 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[inline]
fn clamp_u8(v: u8, lo: u8, hi: u8) -> u8 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[inline]
fn blend_channel(src: u8, a: u8, bg: u8) -> u8 {
    // out = src*a + bg*(1-a)
    // using integer math with rounding
    let src = src as u16;
    let a = a as u16;
    let bg = bg as u16;
    let out = (src * a + bg * (255 - a) + 127) / 255;
    out as u8
}

fn choose_out_mode(mime: &str, ext: &str, out_mode_sel: &str) -> String {
    if out_mode_sel != "auto" {
        return out_mode_sel.to_string();
    }
    if mime.contains("pdf") || ext == "pdf" {
        return "pdf".to_string();
    }
    if mime.contains("png") || ext == "png" {
        return "png".to_string();
    }
    "jpeg".to_string()
}

fn is_pdf(mime: &str, ext: &str, bytes: &[u8]) -> bool {
    if mime.contains("pdf") || ext == "pdf" {
        return true;
    }
    bytes.len() >= 4 && &bytes[0..4] == b"%PDF"
}

fn input_kind(mime: &str, ext: &str) -> &'static str {
    if mime.contains("pdf") || ext == "pdf" {
        return "pdf";
    }
    if mime.contains("png") || ext == "png" {
        return "png";
    }
    if mime.contains("jpg") || mime.contains("jpeg") || ext == "jpg" || ext == "jpeg" {
        return "jpeg";
    }
    "unknown"
}

fn scale_to_max_side(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    if max_side == 0 {
        return (w, h);
    }
    let m = w.max(h);
    if m <= max_side {
        return (w, h);
    }
    let scale = max_side as f32 / m as f32;
    let new_w = (w as f32 * scale).round().max(1.0) as u32;
    let new_h = (h as f32 * scale).round().max(1.0) as u32;
    (new_w, new_h)
}

#[wasm_bindgen]
pub struct CompressionResult {
    bytes: Vec<u8>,
    out_mode: String,
}

impl CompressionResult {
    fn new(bytes: Vec<u8>, out_mode: String) -> Self {
        Self { bytes, out_mode }
    }
}

#[wasm_bindgen]
impl CompressionResult {
    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = outMode)]
    pub fn out_mode(&self) -> String {
        self.out_mode.clone()
    }
}

/// Encode RGBA pixels into a JPEG.
/// - If alpha is present, the image is composited onto the given background color.
#[wasm_bindgen]
pub fn encode_jpeg_rgba(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    quality: u8,
    preset: u8,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) -> Vec<u8> {
    let px = (width as usize).saturating_mul(height as usize);
    if rgba.len() < px.saturating_mul(4) || px == 0 {
        return Vec::new();
    }

    let mut rgb = Vec::with_capacity(px.saturating_mul(3));
    for i in 0..px {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        let b = rgba[i * 4 + 2];
        let a = rgba[i * 4 + 3];

        if a == 255 {
            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        } else if a == 0 {
            rgb.push(bg_r);
            rgb.push(bg_g);
            rgb.push(bg_b);
        } else {
            rgb.push(blend_channel(r, a, bg_r));
            rgb.push(blend_channel(g, a, bg_g));
            rgb.push(blend_channel(b, a, bg_b));
        }
    }

    let quality = clamp_u8(quality, 1, 100);
    let preset = clamp_u8(preset, 0, 2);
    let mut opts = JpegOptions::from_preset(width, height, quality, preset);
    opts.color_type = ColorType::Rgb;

    jpeg::encode(&rgb, &opts).unwrap_or_default()
}

/// Encode RGBA pixels into a PNG.
/// - `lossless=true` keeps full RGBA (and applies lossless optimizations).
/// - `lossless=false` enables palette quantization ("lossy PNG") for big savings on screenshots/UI.
#[wasm_bindgen]
pub fn encode_png_rgba(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    preset: u8,
    lossless: bool,
    max_colors: u16,
    dithering: bool,
    force_quant: bool,
) -> Vec<u8> {
    let px = (width as usize).saturating_mul(height as usize);
    if rgba.len() < px.saturating_mul(4) || px == 0 {
        return Vec::new();
    }

    let preset = clamp_u8(preset, 0, 2);
    let mut opts = PngOptions::from_preset_with_lossless(width, height, preset, lossless);
    opts.color_type = ColorType::Rgba;

    if !lossless {
        opts.quantization.mode = if force_quant {
            QuantizationMode::Force
        } else {
            QuantizationMode::Auto
        };
        opts.quantization.max_colors = clamp_u16(max_colors, 1, 256);
        opts.quantization.dithering = dithering;
    }

    png::encode(&rgba, &opts).unwrap_or_default()
}

const MIN_TARGET_QUALITY: u8 = 1;

fn estimate_quality(max_quality: u8, current_bytes: usize, target_bytes: usize) -> u8 {
    if current_bytes == 0 || target_bytes == 0 {
        return max_quality;
    }
    let ratio = (target_bytes as f32 / current_bytes as f32).clamp(0.05, 1.0);
    let predicted = (max_quality as f32 * ratio.powf(0.6)).round() as i32;
    predicted.clamp(1, max_quality as i32) as u8
}

fn encode_png_with_quality(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    preset: u8,
    png_mode: &str,
    png_max_colors: u16,
    png_dither: bool,
    png_force_quant: bool,
) -> Vec<u8> {
    let auto_level = clamp_u8(quality, 1, 100);
    let colors_from_level = (8.0 + (auto_level as f32 / 100.0) * 248.0).round() as u16;
    let (lossless, colors, dithering, force_quant) = match png_mode {
        "lossless" => (true, png_max_colors, png_dither, png_force_quant),
        "auto" => (false, colors_from_level, auto_level <= 50, auto_level < 90),
        _ => (false, png_max_colors, png_dither, png_force_quant),
    };

    encode_png_rgba(
        width,
        height,
        rgba.to_vec(),
        preset,
        lossless,
        clamp_u16(colors, 1, 256),
        dithering,
        force_quant,
    )
}

fn compress_png_to_target(
    rgba: &[u8],
    width: u32,
    height: u32,
    preset: u8,
    png_mode: &str,
    png_max_colors: u16,
    png_dither: bool,
    png_force_quant: bool,
    max_quality: u8,
    target_bytes: usize,
) -> Vec<u8> {
    let max_quality = clamp_u8(max_quality, 1, 100);
    let min_quality = MIN_TARGET_QUALITY.min(max_quality);

    let best = encode_png_with_quality(
        rgba,
        width,
        height,
        max_quality,
        preset,
        png_mode,
        png_max_colors,
        png_dither,
        png_force_quant,
    );
    if best.len() <= target_bytes {
        return best;
    }

    let mut lo = min_quality;
    let mut hi = max_quality;
    let mut best_under: Option<Vec<u8>> = None;
    let mut smallest = best.clone();

    let estimate = estimate_quality(max_quality, best.len(), target_bytes);
    if estimate > min_quality && estimate < max_quality {
        hi = estimate;
    }

    let mut iterations = 0;
    while lo <= hi && iterations < 6 {
        let mid = (lo as u16 + hi as u16) / 2;
        let mid_q = mid as u8;
        let out = encode_png_with_quality(
            rgba,
            width,
            height,
            mid_q,
            preset,
            png_mode,
            png_max_colors,
            png_dither,
            png_force_quant,
        );
        if out.len() < smallest.len() {
            smallest = out.clone();
        }
        if out.len() <= target_bytes {
            best_under = Some(out);
            lo = mid_q.saturating_add(1);
        } else {
            if mid_q == 0 {
                break;
            }
            hi = mid_q.saturating_sub(1);
        }
        iterations += 1;
    }

    best_under.unwrap_or(smallest)
}

fn compress_jpeg_to_target(
    rgba: &[u8],
    width: u32,
    height: u32,
    preset: u8,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    max_quality: u8,
    target_bytes: usize,
) -> Vec<u8> {
    let max_quality = clamp_u8(max_quality, 1, 100);
    let min_quality = MIN_TARGET_QUALITY.min(max_quality);

    let best = encode_jpeg_rgba(
        width,
        height,
        rgba.to_vec(),
        max_quality,
        preset,
        bg_r,
        bg_g,
        bg_b,
    );
    if best.len() <= target_bytes {
        return best;
    }

    let mut lo = min_quality;
    let mut hi = max_quality;
    let mut best_under: Option<Vec<u8>> = None;
    let mut smallest = best.clone();

    let estimate = estimate_quality(max_quality, best.len(), target_bytes);
    if estimate > min_quality && estimate < max_quality {
        hi = estimate;
    }

    let mut iterations = 0;
    while lo <= hi && iterations < 6 {
        let mid = (lo as u16 + hi as u16) / 2;
        let mid_q = mid as u8;
        let out = encode_jpeg_rgba(
            width,
            height,
            rgba.to_vec(),
            mid_q,
            preset,
            bg_r,
            bg_g,
            bg_b,
        );
        if out.len() < smallest.len() {
            smallest = out.clone();
        }
        if out.len() <= target_bytes {
            best_under = Some(out);
            lo = mid_q.saturating_add(1);
        } else {
            if mid_q == 0 {
                break;
            }
            hi = mid_q.saturating_sub(1);
        }
        iterations += 1;
    }

    best_under.unwrap_or(smallest)
}

fn compress_pdf_to_target(
    pdf: Vec<u8>,
    max_quality: u8,
    preset: u8,
    target_bytes: usize,
) -> Vec<u8> {
    let max_quality = clamp_u8(max_quality, 1, 100);
    let min_quality = MIN_TARGET_QUALITY.min(max_quality);
    let best = compress_pdf_images(pdf.clone(), max_quality, preset);
    if best.len() <= target_bytes {
        return best;
    }

    let mut lo = min_quality;
    let mut hi = max_quality;
    let mut best_under: Option<Vec<u8>> = None;
    let mut smallest = best.clone();

    let estimate = estimate_quality(max_quality, best.len(), target_bytes);
    if estimate > min_quality && estimate < max_quality {
        hi = estimate;
    }

    let mut iterations = 0;
    while lo <= hi && iterations < 6 {
        let mid = (lo as u16 + hi as u16) / 2;
        let mid_q = mid as u8;
        let out = compress_pdf_images(pdf.clone(), mid_q, preset);
        if out.len() < smallest.len() {
            smallest = out.clone();
        }
        if out.len() <= target_bytes {
            best_under = Some(out);
            lo = mid_q.saturating_add(1);
        } else {
            if mid_q == 0 {
                break;
            }
            hi = mid_q.saturating_sub(1);
        }
        iterations += 1;
    }

    best_under.unwrap_or(smallest)
}

#[wasm_bindgen]
pub fn compress_file(
    bytes: Vec<u8>,
    mime: String,
    ext: String,
    out_mode_sel: String,
    quality: u8,
    preset: u8,
    max_side: u32,
    target_kb: u32,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    png_mode: String,
    png_max_colors: u16,
    png_dither: bool,
    png_force_quant: bool,
    bg_transparent: bool,
) -> Result<CompressionResult, JsValue> {
    if bytes.is_empty() {
        return Ok(CompressionResult::new(Vec::new(), "jpeg".to_string()));
    }

    let orig_bytes = bytes;
    let mime = mime.to_lowercase();
    let ext = ext.to_lowercase();
    let kind = input_kind(&mime, &ext);
    let orig_len = orig_bytes.len();
    let is_pdf_input = is_pdf(&mime, &ext, &orig_bytes);
    let mut out_mode = choose_out_mode(&mime, &ext, &out_mode_sel);
    if is_pdf_input {
        out_mode = "pdf".to_string();
    } else if out_mode == "pdf" {
        out_mode = choose_out_mode(&mime, &ext, "auto");
    }
    if bg_transparent && out_mode == "jpeg" && kind == "png" {
        out_mode = "png".to_string();
    }

    let quality = clamp_u8(quality, 1, 100);
    let preset = clamp_u8(preset, 0, 2);
    let target_bytes = target_kb.saturating_mul(1024) as usize;

    if out_mode == "pdf" {
        let out = if target_bytes > 0 {
            compress_pdf_to_target(orig_bytes, quality, preset, target_bytes)
        } else {
            compress_pdf_images(orig_bytes, quality, preset)
        };
        return Ok(CompressionResult::new(out, "pdf".to_string()));
    }

    let img = image::load_from_memory(&orig_bytes)
        .map_err(|e| JsValue::from_str(&format!("Decode failed: {e}")))?;
    let mut rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let (new_w, new_h) = scale_to_max_side(w, h, max_side);
    let resized = new_w != w || new_h != h;
    if new_w != w || new_h != h {
        rgba = image::imageops::resize(&rgba, new_w, new_h, FilterType::Lanczos3);
    }
    let (width, height) = rgba.dimensions();
    let rgba_raw = rgba.into_raw();

    if out_mode == "png" {
        let png_mode = png_mode.to_lowercase();
        let enc_bytes = if target_bytes > 0 {
            compress_png_to_target(
                &rgba_raw,
                width,
                height,
                preset,
                &png_mode,
                png_max_colors,
                png_dither,
                png_force_quant,
                quality,
                target_bytes,
            )
        } else {
            encode_png_with_quality(
                &rgba_raw,
                width,
                height,
                quality,
                preset,
                &png_mode,
                png_max_colors,
                png_dither,
                png_force_quant,
            )
        };
        if target_bytes == 0 && !resized && kind == "png" && enc_bytes.len() >= orig_len {
            return Ok(CompressionResult::new(orig_bytes, "png".to_string()));
        }
        return Ok(CompressionResult::new(enc_bytes, "png".to_string()));
    }

    let enc_bytes = if target_bytes > 0 {
        compress_jpeg_to_target(
            &rgba_raw,
            width,
            height,
            preset,
            bg_r,
            bg_g,
            bg_b,
            quality,
            target_bytes,
        )
    } else {
        encode_jpeg_rgba(width, height, rgba_raw, quality, preset, bg_r, bg_g, bg_b)
    };
    if target_bytes == 0 && !resized && kind == "jpeg" && enc_bytes.len() >= orig_len {
        return Ok(CompressionResult::new(orig_bytes, "jpeg".to_string()));
    }
    Ok(CompressionResult::new(enc_bytes, "jpeg".to_string()))
}

#[wasm_bindgen]
pub fn compress_pdf_images(pdf: Vec<u8>, quality: u8, preset: u8) -> Vec<u8> {
    if pdf.is_empty() {
        return Vec::new();
    }

    let mut doc = match Document::load_mem(&pdf) {
        Ok(doc) => doc,
        Err(_) => return pdf,
    };

    let quality = clamp_u8(quality, 1, 100);
    let preset = clamp_u8(preset, 0, 2);
    let mut changed = false;

    for (_, obj) in doc.objects.iter_mut() {
        let stream = match obj {
            Object::Stream(stream) => stream,
            _ => continue,
        };

        let subtype = stream.dict.get(b"Subtype").and_then(Object::as_name).ok();
        if subtype != Some(b"Image") {
            continue;
        }

        let filters = match stream.filters() {
            Ok(filters) => filters,
            Err(_) => continue,
        };
        if filters.len() != 1 || filters[0] != b"FlateDecode" {
            continue;
        }

        let width = match stream.dict.get(b"Width").and_then(Object::as_i64) {
            Ok(value) if value > 0 => value as usize,
            _ => continue,
        };
        let height = match stream.dict.get(b"Height").and_then(Object::as_i64) {
            Ok(value) if value > 0 => value as usize,
            _ => continue,
        };
        let bits = match stream
            .dict
            .get(b"BitsPerComponent")
            .and_then(Object::as_i64)
        {
            Ok(value) => value,
            _ => 8,
        };
        if bits != 8 {
            continue;
        }

        let color_space = match stream.dict.get(b"ColorSpace") {
            Ok(Object::Name(name)) if name == b"DeviceRGB" => "rgb",
            Ok(Object::Name(name)) if name == b"DeviceGray" => "gray",
            _ => continue,
        };

        let decoded = match stream.decompressed_content() {
            Ok(data) => data,
            Err(_) => continue,
        };

        let px = match width.checked_mul(height) {
            Some(px) if px > 0 => px,
            _ => continue,
        };

        let rgb = if color_space == "rgb" {
            let expected = match px.checked_mul(3) {
                Some(value) => value,
                None => continue,
            };
            if decoded.len() != expected {
                continue;
            }
            decoded
        } else {
            if decoded.len() != px {
                continue;
            }
            let mut out = Vec::with_capacity(px * 3);
            for v in decoded {
                out.push(v);
                out.push(v);
                out.push(v);
            }
            out
        };

        let mut opts = JpegOptions::from_preset(width as u32, height as u32, quality, preset);
        opts.color_type = ColorType::Rgb;
        let jpeg_bytes = jpeg::encode(&rgb, &opts).unwrap_or_default();
        if jpeg_bytes.is_empty() {
            continue;
        }

        stream.set_content(jpeg_bytes);
        stream
            .dict
            .set("Filter", Object::Name(b"DCTDecode".to_vec()));
        stream
            .dict
            .set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        stream.dict.set("BitsPerComponent", 8);
        stream.dict.remove(b"DecodeParms");
        changed = true;
    }

    if !changed {
        return pdf;
    }

    let mut out = Vec::new();
    if doc.save_to(&mut out).is_ok() {
        out
    } else {
        pdf
    }
}
