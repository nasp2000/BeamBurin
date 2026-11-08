use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use crate::errors::{BeamError, Result};
use crate::objects::{ImageMode, ImageSettings};

/// Maximum number of pixels accepted from an imported image.
pub const MAX_PIXELS: u64 = 50_000_000;

/// Largest side processed for an on-screen preview.
///
/// The canvas texture is much smaller than the source, so damping the preview
/// before the tone and dithering passes keeps slider changes responsive without
/// affecting the full-resolution job path. 1024 px keeps the large photo sharp
/// on screen while the bounded cache makes repeated frames cheap.
pub const PREVIEW_MAX_SIDE: u32 = 1024;

/// Largest side processed before vectorizing or tracing a photo.
pub const TRACE_MAX_SIDE: u32 = 1024;

/// Power map produced from an image and its settings.
///
/// Most modes are binary and store only 0 or 255; grayscale engraving stores the
/// prepared tone as the power of every pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterBitmap {
    pub width: usize,
    pub height: usize,
    /// Laser power per pixel: 0 does not burn, 255 is full power.
    pub power: Vec<u8>,
}

impl RasterBitmap {
    pub fn burned(&self) -> usize {
        self.power.iter().filter(|value| **value > 0).count()
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        self.power_at(x, y) > 0
    }

    pub fn power_at(&self, x: usize, y: usize) -> u8 {
        self.power[y * self.width + x]
    }
}

/// Counts used to estimate how long an engraving takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RasterStats {
    pub width: usize,
    pub height: usize,
    /// Pixels whose power is above zero.
    pub burned: usize,
    /// Burned runs, one per uninterrupted stretch in a row.
    pub runs: usize,
    /// Unburned pixels between the first and last burned pixel of a row.
    pub gap_pixels: usize,
}

impl RasterStats {
    pub fn of(bitmap: &RasterBitmap) -> Self {
        let mut stats = Self {
            width: bitmap.width,
            height: bitmap.height,
            ..Self::default()
        };
        for row in 0..bitmap.height {
            let mut first: Option<usize> = None;
            let mut last = 0;
            let mut row_burned = 0;
            let mut previous = false;
            for column in 0..bitmap.width {
                let on = bitmap.get(column, row);
                if on {
                    stats.burned += 1;
                    row_burned += 1;
                    first.get_or_insert(column);
                    last = column;
                    if !previous {
                        stats.runs += 1;
                    }
                }
                previous = on;
            }
            if let Some(first) = first {
                stats.gap_pixels += (last - first + 1) - row_burned;
            }
        }
        stats
    }

    pub fn is_empty(&self) -> bool {
        self.burned == 0
    }

    /// Burned share with one decimal and the number of burned runs.
    ///
    /// Integer per-mille keeps the text exact for the interface and avoids a
    /// floating-point cast.
    pub fn coverage_text(&self) -> String {
        let total = self.width.saturating_mul(self.height);
        let per_mille = self
            .burned
            .saturating_mul(1000)
            .checked_div(total)
            .unwrap_or(0);
        let runs = if self.runs == 1 {
            "1 run".to_string()
        } else {
            format!("{} runs", self.runs)
        };
        format!("{}.{}% burned, {runs}", per_mille / 10, per_mille % 10)
    }
}

/// Rough engraving time from the processed pixels.
///
/// Cut length is the burned pixels times the pixel width; travel is the
/// unburned pixels between runs. Row changes and acceleration are ignored, so
/// the result is an estimate, not a promise.
#[allow(clippy::cast_precision_loss)]
pub fn estimate_engraving(
    stats: RasterStats,
    width_mm: f64,
    speed_mm_min: f64,
) -> Option<(f64, f64, f64)> {
    if stats.width == 0 || !width_mm.is_finite() || width_mm <= 0.0 || speed_mm_min <= 0.0 {
        return None;
    }
    let pixel_mm = width_mm / stats.width as f64;
    let cut_mm = stats.burned as f64 * pixel_mm;
    let travel_mm = stats.gap_pixels as f64 * pixel_mm;
    let seconds = (cut_mm + travel_mm) / speed_mm_min * 60.0;
    Some((cut_mm, travel_mm, seconds))
}

/// Convert a luminance buffer using the image settings.
///
/// Pipeline: black/white clipping, brightness, contrast, tone curve (shadows,
/// midtones, highlights), gamma, enhancement, invert, then threshold or
/// dithering. The tone curve runs before gamma so the three controls shape the
/// tonal range rather than undoing the gamma correction. With all three at zero
/// the curve is an exact identity and the pipeline is unchanged.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn process_luma(
    luma: &[u8],
    width: usize,
    height: usize,
    settings: &ImageSettings,
) -> Result<RasterBitmap> {
    process_luma_masked(luma, width, height, settings, None)
}

/// Process luminance with an optional exclusion mask.
///
/// An excluded pixel is forced to zero power after dithering, so a colour mask
/// holds for every mode, including inverted and grayscale runs. `exclude` must
/// have one entry per pixel when it is present.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn process_luma_masked(
    luma: &[u8],
    width: usize,
    height: usize,
    settings: &ImageSettings,
    exclude: Option<&[bool]>,
) -> Result<RasterBitmap> {
    if width == 0 || height == 0 {
        return Err(BeamError::InvalidInput("image has no pixels".to_string()));
    }
    if luma.len() != width * height {
        return Err(BeamError::InvalidInput(
            "image buffer does not match its dimensions".to_string(),
        ));
    }
    settings.validate().map_err(BeamError::InvalidInput)?;
    if let Some(exclude) = exclude
        && exclude.len() != width * height
    {
        return Err(BeamError::InvalidInput(
            "the colour mask does not match the image".to_string(),
        ));
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let quantize = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    // One byte per pixel instead of four keeps the peak allocation of a large
    // image low enough for old machines.
    let mut adjusted: Vec<u8> = luma
        .iter()
        .map(|value| quantize(adjust(f32::from(*value), settings)))
        .collect();
    if settings.enhance_radius_px > 0 && settings.enhance_amount > 0.0 {
        enhance(
            &mut adjusted,
            width,
            height,
            settings.enhance_radius_px as usize,
            settings.enhance_amount,
            settings.enhance_denoise,
        );
    }

    let mut power = match settings.mode {
        ImageMode::Threshold => adjusted
            .iter()
            .map(|value| if *value < settings.threshold { 255 } else { 0 })
            .collect(),
        ImageMode::FloydSteinberg => diffuse(&adjusted, width, height, &FLOYD_STEINBERG, 16),
        ImageMode::Atkinson => diffuse(&adjusted, width, height, &ATKINSON, 8),
        ImageMode::Stucki => diffuse(&adjusted, width, height, &STUCKI, 42),
        ImageMode::Jarvis => diffuse(&adjusted, width, height, &JARVIS, 48),
        ImageMode::Sierra => diffuse(&adjusted, width, height, &SIERRA, 32),
        ImageMode::Burkes => diffuse(&adjusted, width, height, &BURKES, 32),
        ImageMode::OrderedDither => {
            let mut power = vec![0u8; adjusted.len()];
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;
                    let limit = (f32::from(BAYER_8[y % 8][x % 8]) + 0.5) * 255.0 / 64.0;
                    power[index] = if f32::from(adjusted[index]) < limit {
                        255
                    } else {
                        0
                    };
                }
            }
            power
        }
        ImageMode::Halftone => {
            let mut power = vec![0u8; adjusted.len()];
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;
                    let limit = (f32::from(CLUSTER_4[y % 4][x % 4]) + 0.5) * 255.0 / 16.0;
                    power[index] = if f32::from(adjusted[index]) < limit {
                        255
                    } else {
                        0
                    };
                }
            }
            power
        }
        ImageMode::Newsprint => {
            let mut power = vec![0u8; adjusted.len()];
            let (sin, cos) = settings.halftone_angle_degrees.to_radians().sin_cos();
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;
                    let rx = (x as f64 * cos + y as f64 * sin).rem_euclid(8.0) as usize;
                    let ry = (-(x as f64) * sin + y as f64 * cos).rem_euclid(8.0) as usize;
                    let u = (rx + ry) % 8;
                    let v = (rx + 2 * ry) % 8;
                    let limit = (f32::from(BAYER_8[u][v]) + 0.5) * 255.0 / 64.0;
                    power[index] = if f32::from(adjusted[index]) < limit {
                        255
                    } else {
                        0
                    };
                }
            }
            power
        }
        ImageMode::Sketch => sketch(&adjusted, width, height, settings.threshold),
        // Grayscale keeps the prepared tone as the power.
        ImageMode::Grayscale => adjusted.iter().map(|value| 255 - value).collect(),
    };
    if let Some(exclude) = exclude {
        for (value, skipped) in power.iter_mut().zip(exclude) {
            if *skipped {
                *value = 0;
            }
        }
    }
    Ok(RasterBitmap {
        width,
        height,
        power,
    })
}

const BAYER_8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Clustered-dot screen used by the halftone modes.
const CLUSTER_4: [[u8; 4]; 4] = [[12, 5, 6, 13], [4, 0, 1, 7], [11, 3, 2, 8], [15, 10, 9, 14]];

const FLOYD_STEINBERG: [(i32, i32, i32); 4] = [(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)];

const ATKINSON: [(i32, i32, i32); 6] = [
    (1, 0, 1),
    (2, 0, 1),
    (-1, 1, 1),
    (0, 1, 1),
    (1, 1, 1),
    (0, 2, 1),
];

const STUCKI: [(i32, i32, i32); 12] = [
    (1, 0, 8),
    (2, 0, 4),
    (-2, 1, 2),
    (-1, 1, 4),
    (0, 1, 8),
    (1, 1, 4),
    (2, 1, 2),
    (-2, 2, 1),
    (-1, 2, 2),
    (0, 2, 4),
    (1, 2, 2),
    (2, 2, 1),
];

const SIERRA: [(i32, i32, i32); 10] = [
    (1, 0, 5),
    (2, 0, 3),
    (-2, 1, 2),
    (-1, 1, 4),
    (0, 1, 5),
    (1, 1, 4),
    (2, 1, 2),
    (-1, 2, 2),
    (0, 2, 3),
    (1, 2, 2),
];

const BURKES: [(i32, i32, i32); 7] = [
    (1, 0, 8),
    (2, 0, 4),
    (-2, 1, 2),
    (-1, 1, 4),
    (0, 1, 8),
    (1, 1, 4),
    (2, 1, 2),
];

const JARVIS: [(i32, i32, i32); 12] = [
    (1, 0, 7),
    (2, 0, 5),
    (-2, 1, 3),
    (-1, 1, 5),
    (0, 1, 7),
    (1, 1, 5),
    (2, 1, 3),
    (-2, 2, 1),
    (-1, 2, 3),
    (0, 2, 5),
    (1, 2, 3),
    (2, 2, 1),
];

/// Error diffusion with the given kernel and divisor.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn diffuse(
    adjusted: &[u8],
    width: usize,
    height: usize,
    kernel: &[(i32, i32, i32)],
    divisor: i32,
) -> Vec<u8> {
    // Only three rows of error are needed: every kernel pushes errors to the
    // current row and at most two rows below, so the image itself stays one
    // byte per pixel.
    let mut power = vec![0u8; adjusted.len()];
    let mut errors = [
        vec![0.0_f32; width],
        vec![0.0_f32; width],
        vec![0.0_f32; width],
    ];
    for y in 0..height {
        let row = y % 3;
        for x in 0..width {
            let index = y * width + x;
            let old = f32::from(adjusted[index]) + errors[row][x];
            let new = if old < 128.0 { 0.0 } else { 255.0 };
            power[index] = if new == 0.0 { 255 } else { 0 };
            let error = old - new;
            for (dx, dy, weight) in kernel {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let target = ny as usize % 3;
                    errors[target][nx as usize] += error * (*weight as f32) / (divisor as f32);
                }
            }
        }
        errors[row].fill(0.0);
    }
    power
}

/// Burn the edges found by a Sobel operator. `threshold` scales the gradient
/// that is considered an edge; flat areas stay unburned.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn sketch(adjusted: &[u8], width: usize, height: usize, threshold: u8) -> Vec<u8> {
    let mut power = vec![0u8; adjusted.len()];
    if width < 3 || height < 3 {
        return power;
    }
    let limit = f32::from(threshold) * 4.0;
    let at = |x: usize, y: usize| f32::from(adjusted[y * width + x]);
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let gx = -at(x - 1, y - 1) + at(x + 1, y - 1) - 2.0 * at(x - 1, y) + 2.0 * at(x + 1, y)
                - at(x - 1, y + 1)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            if gx.hypot(gy) > limit {
                power[y * width + x] = 255;
            }
        }
    }
    power
}

/// Unsharp mask: add the difference between the image and a box blur.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn enhance(
    values: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    amount: f32,
    denoise: f32,
) {
    if radius == 0 || amount <= 0.0 {
        return;
    }
    // Up to 32 levels of detail are treated as noise in smooth areas.
    let ignored = denoise.clamp(0.0, 10.0) / 10.0 * 32.0;
    let blurred = box_blur(values, width, height, radius);
    for (value, blurred) in values.iter_mut().zip(blurred) {
        let original = f32::from(*value);
        let detail = original - f32::from(blurred);
        if detail.abs() < ignored {
            continue;
        }
        let sharpened = original + amount * detail;
        *value = sharpened.round().clamp(0.0, 255.0) as u8;
    }
}

/// Sliding-window box blur on one-byte values, with integer accumulators.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn box_blur(values: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 || width == 0 || height == 0 {
        return values.to_vec();
    }
    let mut horizontal = vec![0u8; values.len()];
    let last = width - 1;
    for y in 0..height {
        let row = y * width;
        let initial_end = radius.min(last);
        let mut sum: u32 = (0..=initial_end).map(|x| u32::from(values[row + x])).sum();
        for x in 0..width {
            let left = x.saturating_sub(radius);
            let right = (x + radius).min(last);
            let count = (right - left + 1) as u32;
            horizontal[row + x] = ((sum + count / 2) / count) as u8;
            if x + radius + 1 < width {
                sum += u32::from(values[row + x + radius + 1]);
            }
            if x >= radius {
                sum -= u32::from(values[row + x - radius]);
            }
        }
    }
    let mut out = vec![0u8; values.len()];
    let last_row = height - 1;
    for x in 0..width {
        let initial_end = radius.min(last_row);
        let mut sum: u32 = (0..=initial_end)
            .map(|y| u32::from(horizontal[y * width + x]))
            .sum();
        for y in 0..height {
            let top = y.saturating_sub(radius);
            let bottom = (y + radius).min(last_row);
            let count = (bottom - top + 1) as u32;
            out[y * width + x] = ((sum + count / 2) / count) as u8;
            if y + radius + 1 < height {
                sum += u32::from(horizontal[(y + radius + 1) * width + x]);
            }
            if y >= radius {
                sum -= u32::from(horizontal[(y - radius) * width + x]);
            }
        }
    }
    out
}

fn adjust(value: f32, settings: &ImageSettings) -> f32 {
    let black = f32::from(settings.black_clip);
    let white = f32::from(settings.white_clip);
    let mut value = ((value - black) / (white - black) * 255.0).clamp(0.0, 255.0);
    value += f32::from(settings.brightness) * 2.55;
    value = (value - 128.0) * settings.contrast + 128.0;
    value = value.clamp(0.0, 255.0);
    value = tone_curve(value, settings);
    value = 255.0 * (value.clamp(0.0, 255.0) / 255.0).powf(1.0 / settings.gamma);
    if settings.invert {
        value = 255.0 - value;
    }
    value.clamp(0.0, 255.0)
}

/// Largest shift, in tone levels, that one tone-curve control can apply.
///
/// The three smooth weights below sum to one, so with every control at its
/// limit the offset is still bounded. The value is chosen so the curve remains
/// strictly increasing even when two controls pull in opposite directions:
/// the steepest possible slope of the offset is `4 * TONE_LIFT` (192), which is
/// below the identity slope of 255.
const TONE_LIFT: f32 = 48.0;

/// Smooth, monotonic tonal correction with shadow, midtone and highlight lifts.
///
/// The weights are the quadratic Bernstein basis on the normalised value:
/// `(1 - t)²` reaches its maximum at black, `2t(1 - t)` at mid-grey and `t²` at
/// white. They are smooth and sum to one, so a control affects its own range
/// most and the identity at zero is exact. The offset is added to the value and
/// clamped to 0..=255.
fn tone_curve(value: f32, settings: &ImageSettings) -> f32 {
    let shadow = settings.shadow_lift;
    let midtone = settings.midtone_shift;
    let highlight = settings.highlight_lift;
    if shadow == 0.0 && midtone == 0.0 && highlight == 0.0 {
        return value;
    }
    let t = (value / 255.0).clamp(0.0, 1.0);
    let dark_weight = (1.0 - t) * (1.0 - t);
    let mid_weight = 2.0 * t * (1.0 - t);
    let bright_weight = t * t;
    let offset =
        (shadow * dark_weight + midtone * mid_weight + highlight * bright_weight) * TONE_LIFT;
    (value + offset).clamp(0.0, 255.0)
}

/// Pixel dimensions of an image file, without decoding all its pixels twice.
pub fn image_dimensions(path: &std::path::Path) -> Result<(u32, u32)> {
    let reader = image::ImageReader::open(path)
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))?;
    let reader = reader
        .with_guessed_format()
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))?;
    reader
        .into_dimensions()
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))
}

/// Load an image and apply the object's crop, rotation and working size.
///
/// The pipeline is non-destructive: the file is never written. Crop and rotation
/// happen before the tone and dithering settings, and resampling uses nearest
/// neighbour so a working resolution never invents grey detail.
pub fn process_object(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
) -> Result<RasterBitmap> {
    let prepared = prepared_image(path, image, None)?;
    let width = usize::try_from(prepared.luma.width()).unwrap_or(0);
    let height = usize::try_from(prepared.luma.height()).unwrap_or(0);
    process_luma_masked(
        prepared.luma.as_raw(),
        width,
        height,
        &image.settings,
        prepared.exclude.as_deref(),
    )
}

/// Process an image at a bounded preview resolution.
///
/// Source files, crops and rotations are still read at full size, but the
/// luminance buffer passed to the tone and dithering pipeline is limited to
/// `max_side` pixels on its longer edge. This is for previews and tracing only;
/// [`process_object`] keeps the full resolution for the job.
pub fn process_object_preview(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: u32,
) -> Result<RasterBitmap> {
    let prepared = prepared_image(path, image, Some(max_side.max(1)))?;
    let width = usize::try_from(prepared.luma.width()).unwrap_or(0);
    let height = usize::try_from(prepared.luma.height()).unwrap_or(0);
    process_luma_masked(
        prepared.luma.as_raw(),
        width,
        height,
        &image.settings,
        prepared.exclude.as_deref(),
    )
}

/// Cached bounded source, keyed by everything that changes the decoded pixels.
///
/// The canvas preview, the histogram and auto levels all ask for the same
/// bounded image within one frame, and the image dialog changes the grey mix
/// and despeckle while the photo stays open. JPEG decoding dominates all of
/// those calls, so the bounded RGB source is kept until the file, crop, flips,
/// rotation or working size change; the grey conversion, despeckle, tone and
/// dithering rerun on that buffer. Full-resolution jobs are never cached.
struct PreviewCache {
    key: String,
    rgb: Arc<image::RgbImage>,
    /// Grey weights and despeckle, which change the luminance but not the RGB.
    luma_key: String,
    luma: Arc<image::GrayImage>,
    exclude: Option<Vec<bool>>,
}

/// Settings that change the luminance derived from the cached RGB.
fn derived_luma_key(settings: &ImageSettings) -> String {
    format!(
        "{}|{}|{}|{}",
        settings.gray_red.to_bits(),
        settings.gray_green.to_bits(),
        settings.gray_blue.to_bits(),
        settings.despeckle
    )
}

fn preview_cache() -> &'static Mutex<Option<PreviewCache>> {
    static CACHE: OnceLock<Mutex<Option<PreviewCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn preview_key(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: u32,
) -> String {
    let metadata = std::fs::metadata(path).ok();
    let length = metadata.as_ref().map_or(0, std::fs::Metadata::len);
    let modified = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    let crop = image.crop.map_or((0, 0, 0, 0), |crop| {
        (crop.x, crop.y, crop.width, crop.height)
    });
    let settings = &image.settings;
    let mut key = String::new();
    let mask_key = format!("{:?}", image.mask_paths);
    let _ = write!(
        key,
        "{}|{length}|{modified}|{}|{}|{}|{crop:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{max_side}",
        path.display(),
        image.width_px,
        image.height_px,
        settings.rotation_degrees.to_bits(),
        settings.target_width_px,
        settings.target_height_px,
        settings.flip_horizontal,
        settings.flip_vertical,
        image.mask_colours.join(","),
        image.mask_tolerance,
        mask_key,
        image.mask_invert,
        image.mask_alpha,
        image.mask_alpha_threshold,
    );
    key
}

/// Bounded source and its optional colour-mask exclusion buffer.
struct PreparedImage {
    luma: Arc<image::GrayImage>,
    exclude: Option<Vec<bool>>,
}

/// Decode the source and apply flips, crop, rotation and working size.
fn load_prepared_picture(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: Option<u32>,
) -> Result<image::DynamicImage> {
    // Check the source header before decoding: crop, rotation and a smaller
    // working size do not reduce the memory the decoder needs for the file.
    let (width, height) = image_dimensions(path)?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PIXELS {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {MAX_PIXELS}"
        )));
    }
    let mut picture = image::open(path)
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))?;
    if image.settings.flip_horizontal {
        picture = picture.fliph();
    }
    if image.settings.flip_vertical {
        picture = picture.flipv();
    }
    if let Some(crop) = image.crop {
        picture = picture.crop_imm(crop.x, crop.y, crop.width, crop.height);
    }
    let rotation = image.settings.rotation_degrees;
    picture = if (rotation - 90.0).abs() < 0.5 {
        picture.rotate90()
    } else if (rotation - 180.0).abs() < 0.5 {
        picture.rotate180()
    } else if (rotation - 270.0).abs() < 0.5 {
        picture.rotate270()
    } else {
        picture
    };
    if image.settings.target_width_px > 0 && image.settings.target_height_px > 0 {
        picture = picture.resize_exact(
            image.settings.target_width_px,
            image.settings.target_height_px,
            image::imageops::FilterType::Nearest,
        );
    }
    if let Some(max_side) = max_side {
        if picture.width() > max_side || picture.height() > max_side {
            picture = picture.resize(max_side, max_side, image::imageops::FilterType::Triangle);
        }
    }
    let pixels = u64::from(picture.width()) * u64::from(picture.height());
    if pixels > MAX_PIXELS {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {MAX_PIXELS}"
        )));
    }
    Ok(picture)
}

/// Apply crop, flips, rotation and working size, returning the prepared source
/// and the colour-mask exclusion buffer.
fn prepared_image(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: Option<u32>,
) -> Result<PreparedImage> {
    image.validate().map_err(BeamError::InvalidInput)?;
    let key = max_side.map(|max_side| preview_key(path, image, max_side));
    let mask_colours = image.mask_rgb();
    let settings_key = derived_luma_key(&image.settings);
    if let Some(key) = &key
        && let Some((rgb, exclude, cached_luma)) = preview_cache().lock().ok().and_then(|cache| {
            let entry = cache.as_ref()?;
            if &entry.key != key {
                return None;
            }
            let luma = if entry.luma_key == settings_key {
                Some(Arc::clone(&entry.luma))
            } else {
                None
            };
            Some((Arc::clone(&entry.rgb), entry.exclude.clone(), luma))
        })
    {
        let luma = if let Some(luma) = cached_luma {
            luma
        } else {
            let mut luma = luma_from_rgb(&rgb, &image.settings);
            if image.settings.despeckle {
                luma = despeckle(&luma);
            }
            let luma = Arc::new(luma);
            if let Ok(mut cache) = preview_cache().lock()
                && let Some(entry) = cache.as_mut()
                && &entry.key == key
            {
                entry.luma_key.clone_from(&settings_key);
                entry.luma = Arc::clone(&luma);
            }
            luma
        };
        return Ok(PreparedImage { luma, exclude });
    }
    let picture = load_prepared_picture(path, image, max_side)?;
    let alpha_exclude = alpha_mask_exclusion(&picture, image);
    let prepared_width = usize::try_from(picture.width()).unwrap_or(0);
    let prepared_height = usize::try_from(picture.height()).unwrap_or(0);
    let mut cache_rgb: Option<Arc<image::RgbImage>> = None;
    let (mut luma, colour_exclude) = if max_side.is_some() || !mask_colours.is_empty() {
        let rgb = Arc::new(picture.to_rgb8());
        drop(picture);
        let exclude = colour_exclusion(&rgb, &mask_colours, image.mask_tolerance);
        let luma = luma_from_rgb(&rgb, &image.settings);
        cache_rgb = Some(rgb);
        (luma, exclude)
    } else if uses_default_gray(&image.settings) {
        let luma = picture.to_luma8();
        drop(picture);
        (luma, None)
    } else {
        let rgb = picture.to_rgb8();
        drop(picture);
        (luma_with_weights(&rgb, &image.settings), None)
    };
    if image.settings.despeckle {
        luma = despeckle(&luma);
    }
    let exclude = combine_exclusions(
        combine_exclusions(colour_exclude, alpha_exclude),
        vector_mask_exclusion(
            &image.mask_paths,
            image.mask_invert,
            prepared_width,
            prepared_height,
            image.width_mm(),
            image.height_mm(),
        ),
    );
    let luma = Arc::new(luma);
    if max_side.is_some()
        && let Some(key) = key
        && let Some(rgb) = cache_rgb
        && let Ok(mut cache) = preview_cache().lock()
    {
        *cache = Some(PreviewCache {
            key,
            rgb,
            luma_key: settings_key,
            luma: Arc::clone(&luma),
            exclude: exclude.clone(),
        });
    }
    Ok(PreparedImage { luma, exclude })
}

/// Mark pixels below the alpha threshold when the source has an alpha channel.
fn alpha_mask_exclusion(
    picture: &image::DynamicImage,
    image: &crate::objects::ImageObject,
) -> Option<Vec<bool>> {
    if !image.mask_alpha || !picture.color().has_alpha() {
        return None;
    }
    let rgba = picture.to_rgba8();
    Some(
        rgba.pixels()
            .map(|pixel| pixel[3] < image.mask_alpha_threshold)
            .collect(),
    )
}

/// Merge two exclusion buffers; a pixel is excluded if either source excludes it.
fn combine_exclusions(
    mut first: Option<Vec<bool>>,
    second: Option<Vec<bool>>,
) -> Option<Vec<bool>> {
    match (&mut first, second) {
        (Some(first), Some(second)) => {
            for (left, right) in first.iter_mut().zip(second) {
                *left |= right;
            }
        }
        (None, Some(second)) => first = Some(second),
        _ => {}
    }
    first
}

/// Mark pixels outside the closed vector mask, in image-local millimetres.
#[allow(clippy::cast_precision_loss)]
fn vector_mask_exclusion(
    paths: &[crate::geometry::Path],
    invert: bool,
    width: usize,
    height: usize,
    width_mm: f64,
    height_mm: f64,
) -> Option<Vec<bool>> {
    if paths.is_empty() || width == 0 || height == 0 || width_mm <= 0.0 || height_mm <= 0.0 {
        return None;
    }
    let polygons: Vec<Vec<(f64, f64)>> = paths
        .iter()
        .filter(|path| path.is_closed())
        .map(|path| {
            path.flatten(0.2)
                .into_iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<(f64, f64)>>()
        })
        .filter(|polygon| polygon.len() >= 3)
        .collect();
    if polygons.is_empty() {
        return None;
    }
    let mut exclude = vec![false; width * height];
    for y in 0..height {
        let local_y = height_mm * (1.0 - (y as f64 + 0.5) / height as f64);
        for x in 0..width {
            let local_x = width_mm * (x as f64 + 0.5) / width as f64;
            let inside = polygons.iter().fold(false, |acc, polygon| {
                acc ^ crate::geometry::polygon::contains_point(polygon, local_x, local_y)
            });
            exclude[y * width + x] = inside == invert;
        }
    }
    Some(exclude)
}

fn uses_default_gray(settings: &ImageSettings) -> bool {
    (settings.gray_red - 0.299).abs() < 1e-4
        && (settings.gray_green - 0.587).abs() < 1e-4
        && (settings.gray_blue - 0.114).abs() < 1e-4
}

/// Convert a bounded RGB source to luminance, using the default or user weights.
fn luma_from_rgb(picture: &image::RgbImage, settings: &ImageSettings) -> image::GrayImage {
    if uses_default_gray(settings) {
        use image::Pixel as _;
        return image::GrayImage::from_fn(picture.width(), picture.height(), |x, y| {
            image::Luma([picture.get_pixel(x, y).to_luma()[0]])
        });
    }
    luma_with_weights(picture, settings)
}

/// Mark pixels close to a mask colour as excluded, or `None` without colours.
fn colour_exclusion(
    rgb: &image::RgbImage,
    colours: &[[u8; 3]],
    tolerance: u8,
) -> Option<Vec<bool>> {
    if colours.is_empty() {
        return None;
    }
    Some(
        rgb.pixels()
            .map(|pixel| {
                colours.iter().any(|colour| {
                    pixel[0].abs_diff(colour[0]) <= tolerance
                        && pixel[1].abs_diff(colour[1]) <= tolerance
                        && pixel[2].abs_diff(colour[2]) <= tolerance
                })
            })
            .collect(),
    )
}

/// Convert to luminance with user weights, normalised so they need not sum to 1.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn luma_with_weights(picture: &image::RgbImage, settings: &ImageSettings) -> image::GrayImage {
    let total = (settings.gray_red + settings.gray_green + settings.gray_blue).max(f32::EPSILON);
    image::GrayImage::from_fn(picture.width(), picture.height(), |x, y| {
        let pixel = picture.get_pixel(x, y);
        let value = (f32::from(pixel[0]) * settings.gray_red
            + f32::from(pixel[1]) * settings.gray_green
            + f32::from(pixel[2]) * settings.gray_blue)
            / total;
        image::Luma([value.round().clamp(0.0, 255.0) as u8])
    })
}

/// Remove single-pixel noise with a 3×3 median, leaving the border untouched.
fn despeckle(luma: &image::GrayImage) -> image::GrayImage {
    let (width, height) = luma.dimensions();
    image::GrayImage::from_fn(width, height, |x, y| {
        if x == 0 || y == 0 || x + 1 >= width || y + 1 >= height {
            return *luma.get_pixel(x, y);
        }
        let mut values = [0u8; 9];
        let mut index = 0;
        for dy in 0..3 {
            for dx in 0..3 {
                values[index] = luma.get_pixel(x + dx - 1, y + dy - 1)[0];
                index += 1;
            }
        }
        values.sort_unstable();
        image::Luma([values[4]])
    })
}

/// Luminance histogram of the prepared image, before tone and dithering.
///
/// The 256 bins let the interface show what the tone controls act on, so the
/// clip levels can be judged against the actual image instead of a guess.
pub fn histogram(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
) -> Result<[u64; 256]> {
    let prepared = prepared_image(path, image, None)?;
    Ok(histogram_of_masked(
        prepared.luma.as_raw(),
        prepared.exclude.as_deref(),
    ))
}

/// Histogram of a bounded preview, for the image dialog.
pub fn histogram_preview(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: u32,
) -> Result<[u64; 256]> {
    let prepared = prepared_image(path, image, Some(max_side.max(1)))?;
    Ok(histogram_of_masked(
        prepared.luma.as_raw(),
        prepared.exclude.as_deref(),
    ))
}

/// Count luminance values into 256 bins.
pub fn histogram_of(luma: &[u8]) -> [u64; 256] {
    histogram_of_masked(luma, None)
}

/// Count luminance values, skipping pixels the colour mask excludes.
pub fn histogram_of_masked(luma: &[u8], exclude: Option<&[bool]>) -> [u64; 256] {
    let mut bins = [0u64; 256];
    for (index, value) in luma.iter().enumerate() {
        if exclude.is_some_and(|exclude| exclude.get(index).copied().unwrap_or(false)) {
            continue;
        }
        bins[usize::from(*value)] += 1;
    }
    bins
}

/// Automatic black and white clip levels for the prepared image.
///
/// The lowest and highest half-percent of the histogram are ignored, so a few
/// stray pixels cannot flatten the range. A flat or almost flat image is left
/// unchanged instead of being stretched into noise.
pub fn auto_levels(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
) -> Result<(u8, u8)> {
    let prepared = prepared_image(path, image, None)?;
    Ok(auto_clip_masked(
        prepared.luma.as_raw(),
        prepared.exclude.as_deref(),
    ))
}

/// Automatic clip levels from a bounded preview.
pub fn auto_levels_preview(
    path: &std::path::Path,
    image: &crate::objects::ImageObject,
    max_side: u32,
) -> Result<(u8, u8)> {
    let prepared = prepared_image(path, image, Some(max_side.max(1)))?;
    Ok(auto_clip_masked(
        prepared.luma.as_raw(),
        prepared.exclude.as_deref(),
    ))
}

/// Percentile clip levels for a luminance buffer.
pub fn auto_clip(luma: &[u8]) -> (u8, u8) {
    auto_clip_masked(luma, None)
}

/// Percentile clip levels, ignoring pixels the colour mask excludes.
pub fn auto_clip_masked(luma: &[u8], exclude: Option<&[bool]>) -> (u8, u8) {
    if luma.is_empty() {
        return (0, 255);
    }
    let histogram = histogram_of_masked(luma, exclude);
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return (0, 255);
    }
    let tail = total / 200;
    let mut accumulated = 0u64;
    let mut black = 0u8;
    for (value, count) in histogram.iter().enumerate() {
        accumulated += count;
        if accumulated > tail {
            black = u8::try_from(value).unwrap_or(0);
            break;
        }
    }
    accumulated = 0;
    let mut white = 255u8;
    for (value, count) in histogram.iter().enumerate().rev() {
        accumulated += count;
        if accumulated > tail {
            white = u8::try_from(value).unwrap_or(255);
            break;
        }
    }
    if white.saturating_sub(black) < 8 {
        return (0, 255);
    }
    let black = black.min(240);
    let white = white.max(black.saturating_add(15));
    (black, white)
}

/// Load an image file and produce its burn bitmap.
pub fn process_file(path: &std::path::Path, settings: &ImageSettings) -> Result<RasterBitmap> {
    // The header check avoids allocating the pixel buffer of a file that the
    // limit rejects a moment later.
    let (width, height) = image_dimensions(path)?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PIXELS {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {MAX_PIXELS}"
        )));
    }
    let image = image::open(path)
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))?;
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels > MAX_PIXELS {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {MAX_PIXELS}"
        )));
    }
    let luma = image.to_luma8();
    drop(image);
    process_luma(
        luma.as_raw(),
        luma.width() as usize,
        luma.height() as usize,
        settings,
    )
}

#[cfg(test)]
mod object_tests {
    use super::*;
    use crate::objects::{ImageCrop, ImageObject};

    fn write_image(name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut picture = image::GrayImage::new(width, height);
        for (x, _y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = if x < width / 2 {
                image::Luma([0])
            } else {
                image::Luma([255])
            };
        }
        picture.save(&path).unwrap();
        path
    }

    #[test]
    fn preview_processing_caps_the_long_side() {
        let path = write_image("bw_raster_preview.png", 200, 100);
        let object = ImageObject::new(path.to_string_lossy(), 200, 100, 96.0);
        let preview = process_object_preview(&path, &object, 64).unwrap();
        assert_eq!((preview.width, preview.height), (64, 32));
        let full = process_object(&path, &object).unwrap();
        assert_eq!((full.width, full.height), (200, 100));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn preview_histogram_uses_the_capped_buffer() {
        let path = write_image("bw_raster_preview_histogram.png", 200, 100);
        let object = ImageObject::new(path.to_string_lossy(), 200, 100, 96.0);
        let bins = histogram_preview(&path, &object, 64).unwrap();
        assert_eq!(bins.iter().sum::<u64>(), 64 * 32);
        let (black, white) = auto_levels_preview(&path, &object, 64).unwrap();
        assert!(black <= white);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn changing_the_grey_weights_updates_the_prepared_luma() {
        let path = std::env::temp_dir().join("bw_raster_cache_weights.png");
        let mut picture = image::RgbImage::new(64, 64);
        for (x, _y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = if x < 32 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            };
        }
        picture.save(&path).unwrap();
        let mut object = ImageObject::new(path.to_string_lossy(), 64, 64, 96.0);
        object.settings.gray_red = 1.0;
        object.settings.gray_green = 0.0;
        object.settings.gray_blue = 0.0;
        let red = histogram_preview(&path, &object, 64).unwrap();
        let red_again = histogram_preview(&path, &object, 64).unwrap();
        assert_eq!(red, red_again, "the cached preview changed by itself");
        assert_eq!(red[255], 32 * 64, "the red half should be white");
        assert_eq!(red[0], 32 * 64, "the blue half should be black");
        object.settings.gray_red = 0.5;
        object.settings.gray_blue = 0.5;
        let mixed = histogram_preview(&path, &object, 64).unwrap();
        assert_eq!(mixed[128], 64 * 64, "the cache kept the old grey weights");
        assert_ne!(red, mixed);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_colour_mask_treats_the_colour_as_background() {
        let path = std::env::temp_dir().join("bw_raster_colour_mask.png");
        let mut picture = image::RgbImage::new(8, 4);
        for (x, _y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = if x < 4 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            };
        }
        picture.save(&path).unwrap();
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 4, 96.0);
        let before = histogram_preview(&path, &object, 64).unwrap();
        object.mask_colours = vec!["#ff0000".to_string()];
        object.mask_tolerance = 8;
        let after = histogram_preview(&path, &object, 64).unwrap();
        assert_eq!(
            after.iter().sum::<u64>(),
            16,
            "the masked pixels were counted"
        );
        assert_eq!(after[255], 0, "the red half was not masked");
        assert_ne!(before, after, "the cache kept the unmasked source");
        let bitmap = process_object_preview(&path, &object, 64).unwrap();
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(
                    bitmap.get(x, y),
                    x >= 4,
                    "the colour mask is wrong at {x},{y}"
                );
            }
        }
        object.settings.mode = ImageMode::Grayscale;
        let gray = process_object_preview(&path, &object, 64).unwrap();
        assert_eq!(gray.power_at(0, 0), 0, "grayscale burned the masked colour");
        assert!(
            gray.power_at(7, 0) > 0,
            "grayscale lost the unmasked colour"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_vector_mask_keeps_the_inside_pixels() {
        let path = std::env::temp_dir().join("bw_raster_vector_mask.png");
        let mut picture = image::RgbImage::new(8, 8);
        for pixel in picture.pixels_mut() {
            *pixel = image::Rgb([0, 0, 255]);
        }
        picture.save(&path).unwrap();
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 8, 96.0);
        let width = object.width_mm();
        let height = object.height_mm();
        object.mask_paths = vec![crate::geometry::Path::from_points(
            &[
                (0.0, 0.0),
                (width / 2.0, 0.0),
                (width / 2.0, height),
                (0.0, height),
            ],
            true,
        )];
        let bitmap = process_object_preview(&path, &object, 64).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(bitmap.get(x, y), x < 4, "mask mismatch at {x},{y}");
            }
        }
        object.mask_invert = true;
        let inverted = process_object_preview(&path, &object, 64).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    inverted.get(x, y),
                    x >= 4,
                    "inverted mask mismatch at {x},{y}"
                );
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_alpha_mask_removes_transparent_pixels() {
        let path = std::env::temp_dir().join("bw_raster_alpha_mask.png");
        let mut picture = image::RgbaImage::new(8, 8);
        for (x, _y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = if x < 4 {
                image::Rgba([0, 0, 255, 0])
            } else {
                image::Rgba([0, 0, 255, 255])
            };
        }
        picture.save(&path).unwrap();
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 8, 96.0);
        object.mask_alpha = true;
        object.mask_alpha_threshold = 128;
        let bitmap = process_object_preview(&path, &object, 64).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(bitmap.get(x, y), x >= 4, "alpha mismatch at {x},{y}");
            }
        }
        object.mask_alpha = false;
        let opaque = process_object_preview(&path, &object, 64).unwrap();
        assert!(
            opaque.power.iter().all(|power| *power > 0),
            "the alpha mask leaked into the normal path"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn crop_and_rotation_change_the_bitmap_dimensions() {
        let path = write_image("bw_raster_transform.png", 8, 4);
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 4, 96.0);
        object.crop = Some(ImageCrop {
            x: 2,
            y: 1,
            width: 4,
            height: 2,
        });
        let bitmap = process_object(&path, &object).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (4, 2));
        object.settings.rotation_degrees = 90.0;
        let bitmap = process_object(&path, &object).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (2, 4));
        object.settings.target_width_px = 1;
        object.settings.target_height_px = 2;
        let bitmap = process_object(&path, &object).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (1, 2));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_crop_is_rejected_before_processing() {
        let path = write_image("bw_raster_bad_crop.png", 4, 4);
        let mut object = ImageObject::new(path.to_string_lossy(), 4, 4, 96.0);
        object.crop = Some(ImageCrop {
            x: 3,
            y: 3,
            width: 4,
            height: 4,
        });
        assert!(process_object(&path, &object).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flips_mirror_the_picture() {
        let path = write_image("bw_raster_flip.png", 8, 4);
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 4, 96.0);
        let normal = process_object(&path, &object).unwrap();
        assert!(normal.get(0, 0));
        assert!(!normal.get(7, 0));
        object.settings.flip_horizontal = true;
        let flipped = process_object(&path, &object).unwrap();
        assert!(!flipped.get(0, 0));
        assert!(flipped.get(7, 0));
        object.settings.flip_horizontal = false;
        object.settings.flip_vertical = true;
        let vertical = process_object(&path, &object).unwrap();
        assert_eq!(vertical, normal);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn crop_keeps_the_burned_pixels_of_the_region() {
        let path = write_image("bw_raster_crop_pixels.png", 8, 4);
        let mut object = ImageObject::new(path.to_string_lossy(), 8, 4, 96.0);
        object.crop = Some(ImageCrop {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        });
        let bitmap = process_object(&path, &object).unwrap();
        // The left half is black, so every cropped pixel burns.
        assert!(bitmap.power.iter().all(|value| *value > 0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn gray_weights_choose_the_luminance_channel() {
        let mut picture = image::RgbImage::new(3, 1);
        picture.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        picture.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        picture.put_pixel(2, 0, image::Rgb([0, 0, 255]));
        let mut settings = ImageSettings {
            gray_red: 1.0,
            gray_green: 0.0,
            gray_blue: 0.0,
            ..ImageSettings::default()
        };
        let red = luma_with_weights(&picture, &settings);
        assert_eq!(red.get_pixel(0, 0)[0], 255);
        assert_eq!(red.get_pixel(1, 0)[0], 0);
        settings.gray_red = 0.0;
        settings.gray_green = 0.0;
        settings.gray_blue = 1.0;
        let blue = luma_with_weights(&picture, &settings);
        assert_eq!(blue.get_pixel(2, 0)[0], 255);
        assert_eq!(blue.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn despeckle_removes_a_single_dark_pixel() {
        let mut luma = image::GrayImage::from_pixel(5, 5, image::Luma([255]));
        luma.put_pixel(2, 2, image::Luma([0]));
        let clean = despeckle(&luma);
        assert_eq!(clean.get_pixel(2, 2)[0], 255);
        assert_eq!(clean.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn gray_weights_must_leave_some_light() {
        let mut settings = ImageSettings::default();
        assert!(settings.validate().is_ok());
        settings.gray_red = 0.0;
        settings.gray_green = 0.0;
        settings.gray_blue = 0.0;
        assert!(settings.validate().is_err());
        settings.gray_blue = 1.5;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn histogram_counts_every_prepared_value() {
        let path = std::env::temp_dir().join("bw_raster_histogram.png");
        let mut picture = image::GrayImage::new(8, 1);
        for (x, _y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = image::Luma([u8::try_from(x * 32).unwrap_or(255)]);
        }
        picture.save(&path).unwrap();
        let object = ImageObject::new(path.to_string_lossy(), 8, 1, 96.0);
        let bins = histogram(&path, &object).unwrap();
        assert_eq!(bins[0], 1);
        assert_eq!(bins[32], 1);
        assert_eq!(bins[224], 1);
        assert_eq!(bins.iter().sum::<u64>(), 8);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn histogram_of_an_empty_buffer_is_all_zero() {
        assert_eq!(histogram_of(&[]), [0u64; 256]);
    }

    #[test]
    fn auto_clip_ignores_the_extreme_tails() {
        let mut luma = vec![20u8; 1000];
        luma.extend(std::iter::repeat_n(230u8, 1000));
        luma.push(0);
        luma.push(255);
        let (black, white) = auto_clip(&luma);
        assert!((18..=24).contains(&black), "black clip {black}");
        assert!((226..=232).contains(&white), "white clip {white}");
    }

    #[test]
    fn auto_clip_leaves_a_flat_or_empty_image_alone() {
        assert_eq!(auto_clip(&[]), (0, 255));
        assert_eq!(auto_clip(&[128; 64]), (0, 255));
    }

    #[test]
    fn auto_levels_reads_the_prepared_image() {
        let path = write_image("bw_raster_auto_levels.png", 8, 4);
        let object = ImageObject::new(path.to_string_lossy(), 8, 4, 96.0);
        let (black, white) = auto_levels(&path, &object).unwrap();
        assert_eq!((black, white), (0, 255));
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: ImageMode) -> ImageSettings {
        ImageSettings {
            mode,
            ..ImageSettings::default()
        }
    }

    #[test]
    fn threshold_splits_dark_and_light() {
        let luma = [0u8, 100, 200, 255];
        let bitmap = process_luma(&luma, 2, 2, &settings(ImageMode::Threshold)).unwrap();
        assert!(bitmap.get(0, 0));
        assert!(bitmap.get(1, 0));
        assert!(!bitmap.get(0, 1));
        assert!(!bitmap.get(1, 1));
        assert_eq!(bitmap.burned(), 2);
    }

    #[test]
    fn invert_flips_the_result() {
        let luma = [0u8, 255];
        let mut config = settings(ImageMode::Threshold);
        config.invert = true;
        let bitmap = process_luma(&luma, 2, 1, &config).unwrap();
        assert!(!bitmap.get(0, 0));
        assert!(bitmap.get(1, 0));
    }

    #[test]
    fn brightness_moves_values_across_the_threshold() {
        let luma = [120u8];
        let mut config = settings(ImageMode::Threshold);
        config.threshold = 128;
        assert!(process_luma(&luma, 1, 1, &config).unwrap().get(0, 0));
        config.brightness = 20;
        assert!(!process_luma(&luma, 1, 1, &config).unwrap().get(0, 0));
    }

    #[test]
    fn zero_tone_curve_leaves_a_gradient_unchanged() {
        let config = settings(ImageMode::Threshold);
        for level in 0..=255u8 {
            let adjusted = adjust(f32::from(level), &config);
            assert!(
                (adjusted - f32::from(level)).abs() < 1e-4,
                "level {level} became {adjusted}"
            );
        }
    }

    #[test]
    fn positive_shadow_lift_brightens_dark_pixels_more_than_bright_ones() {
        let mut config = settings(ImageMode::Threshold);
        config.shadow_lift = 1.0;
        let dark = adjust(16.0, &config) - 16.0;
        let bright = adjust(240.0, &config) - 240.0;
        assert!(dark > 30.0, "dark pixels moved by only {dark}");
        assert!(bright < 2.0, "bright pixels moved by {bright}");
        assert!(dark > bright);
    }

    #[test]
    fn positive_highlight_lift_brightens_bright_pixels_more_than_dark_ones() {
        let mut config = settings(ImageMode::Threshold);
        config.highlight_lift = 1.0;
        let dark = adjust(16.0, &config) - 16.0;
        let bright = adjust(200.0, &config) - 200.0;
        assert!(bright > 20.0, "bright pixels moved by only {bright}");
        assert!(dark < 2.0, "dark pixels moved by {dark}");
        assert!(bright > dark);
    }

    #[test]
    fn midtone_shift_moves_a_mid_grey() {
        let mut config = settings(ImageMode::Threshold);
        config.midtone_shift = 1.0;
        let lifted = adjust(128.0, &config);
        assert!((lifted - 152.0).abs() < 0.5, "mid-grey became {lifted}");
        config.midtone_shift = -1.0;
        let lowered = adjust(128.0, &config);
        assert!((lowered - 104.0).abs() < 0.5, "mid-grey became {lowered}");
        // A midtone shift leaves the absolute black and white ends alone.
        assert!(adjust(0.0, &config).abs() < 1e-4);
        assert!((adjust(255.0, &config) - 255.0).abs() < 1e-4);
    }

    #[test]
    fn tone_curves_stay_monotonic_and_clamped() {
        let combinations = [
            (1.0, 1.0, 1.0),
            (-1.0, -1.0, -1.0),
            (1.0, -1.0, 1.0),
            (-1.0, 1.0, -1.0),
            (1.0, -1.0, -1.0),
            (-1.0, 1.0, 1.0),
        ];
        for (shadow, midtone, highlight) in combinations {
            let mut config = settings(ImageMode::Threshold);
            config.shadow_lift = shadow;
            config.midtone_shift = midtone;
            config.highlight_lift = highlight;
            let mut previous = -1.0;
            for level in 0..=255u8 {
                let adjusted = adjust(f32::from(level), &config);
                assert!(
                    (0.0..=255.0).contains(&adjusted),
                    "level {level} left the range: {adjusted}"
                );
                assert!(
                    adjusted >= previous - 1e-4,
                    "curve is not monotonic at level {level}"
                );
                previous = adjusted;
            }
        }
        let mut config = settings(ImageMode::Threshold);
        config.highlight_lift = 1.0;
        assert!((adjust(255.0, &config) - 255.0).abs() < 1e-4);
        config.highlight_lift = 0.0;
        config.shadow_lift = -1.0;
        assert!(adjust(0.0, &config).abs() < 1e-4);
    }

    #[test]
    fn raster_stats_count_runs_and_internal_gaps() {
        let bitmap = super::RasterBitmap {
            width: 4,
            height: 2,
            power: vec![255, 255, 0, 0, 255, 0, 255, 0],
        };
        let stats = super::RasterStats::of(&bitmap);
        assert_eq!(stats.burned, 4);
        assert_eq!(stats.runs, 3);
        assert_eq!(stats.gap_pixels, 1);
        assert!(!stats.is_empty());
    }

    #[test]
    fn coverage_text_reports_the_burned_share_and_runs() {
        let stats = super::RasterStats {
            width: 400,
            height: 100,
            burned: 12_345,
            runs: 7,
            gap_pixels: 0,
        };
        assert_eq!(stats.coverage_text(), "30.8% burned, 7 runs");
        let one = super::RasterStats {
            width: 10,
            height: 10,
            burned: 50,
            runs: 1,
            gap_pixels: 0,
        };
        assert_eq!(one.coverage_text(), "50.0% burned, 1 run");
        let empty = super::RasterStats::default();
        assert_eq!(empty.coverage_text(), "0.0% burned, 0 runs");
    }

    #[test]
    fn the_engraving_estimate_uses_cut_travel_and_speed() {
        let stats = super::RasterStats {
            width: 4,
            height: 1,
            burned: 2,
            runs: 2,
            gap_pixels: 1,
        };
        let (cut, travel, seconds) = super::estimate_engraving(stats, 4.0, 60.0).unwrap();
        assert!((cut - 2.0).abs() < 1e-9);
        assert!((travel - 1.0).abs() < 1e-9);
        assert!((seconds - 3.0).abs() < 1e-9);
        assert!(super::estimate_engraving(stats, 4.0, 0.0).is_none());
        assert!(super::estimate_engraving(stats, 0.0, 60.0).is_none());
    }

    #[test]
    fn enhance_denoise_spares_smooth_noise_but_keeps_edges() {
        let mut noisy = vec![100u8; 16];
        noisy[5] = 105;
        let mut plain = noisy.clone();
        enhance(&mut plain, 4, 4, 1, 1.0, 0.0);
        assert_ne!(plain[5], 105, "the plain enhancement missed the bump");

        let mut quiet = noisy.clone();
        enhance(&mut quiet, 4, 4, 1, 1.0, 10.0);
        assert_eq!(quiet[5], 105, "denoise sharpened the small bump");

        let mut edge = vec![60u8; 16];
        for y in 0..4 {
            edge[y * 4 + 2] = 200;
            edge[y * 4 + 3] = 200;
        }
        let before = edge.clone();
        enhance(&mut edge, 4, 4, 1, 1.0, 10.0);
        assert_ne!(edge, before, "denoise stopped a real edge from sharpening");
    }

    #[test]
    fn the_box_blur_preserves_flat_areas() {
        let values = [77_u8; 5 * 4];
        let blurred = box_blur(&values, 5, 4, 2);
        assert_eq!(blurred, values);
        assert_eq!(box_blur(&values, 5, 4, 0), values);
    }

    #[test]
    fn the_box_blur_matches_a_naive_average() {
        let values: Vec<u8> = (0..7 * 3)
            .map(|index: usize| u8::try_from(index * 17 % 256).unwrap())
            .collect();
        let blurred = box_blur(&values, 7, 3, 1);
        for y in 0..3_usize {
            for x in 0..7_usize {
                let mut sum = 0u32;
                let mut count = 0u32;
                for sy in y.saturating_sub(1)..=(y + 1).min(2) {
                    for sx in x.saturating_sub(1)..=(x + 1).min(6) {
                        sum += u32::from(values[sy * 7 + sx]);
                        count += 1;
                    }
                }
                let expected = u8::try_from((sum + count / 2) / count).unwrap_or(255);
                // The separable pass rounds once between the two directions, so
                // it may differ from the exact 2-D average by one level.
                let difference = (i16::from(blurred[y * 7 + x]) - i16::from(expected)).abs();
                assert!(difference <= 1, "{x},{y}: {difference}");
            }
        }
    }

    #[test]
    fn enhancement_with_zero_amount_changes_nothing() {
        let mut values: Vec<u8> = (0..16)
            .map(|index: usize| u8::try_from(index * 13).unwrap())
            .collect();
        let original = values.clone();
        enhance(&mut values, 4, 4, 2, 0.0, 0.0);
        assert_eq!(values, original);
        enhance(&mut values, 4, 4, 0, 1.0, 0.0);
        assert_eq!(values, original);
    }

    #[test]
    fn grayscale_mode_maps_tone_to_power() {
        let bitmap =
            process_luma(&[0, 64, 192, 255], 4, 1, &settings(ImageMode::Grayscale)).unwrap();
        assert_eq!(bitmap.power_at(0, 0), 255);
        assert_eq!(bitmap.power_at(1, 0), 191);
        assert_eq!(bitmap.power_at(2, 0), 63);
        assert_eq!(bitmap.power_at(3, 0), 0);
        assert!(bitmap.get(0, 0));
        assert!(!bitmap.get(3, 0));
        assert_eq!(bitmap.burned(), 3);
    }

    #[test]
    fn binary_modes_store_only_full_power_or_none() {
        let bitmap =
            process_luma(&[0, 100, 200, 255], 2, 2, &settings(ImageMode::Threshold)).unwrap();
        assert_eq!(bitmap.power, vec![255, 255, 0, 0]);
        let dithered = process_luma(
            &[0, 255, 255, 255],
            2,
            2,
            &settings(ImageMode::FloydSteinberg),
        )
        .unwrap();
        assert!(
            dithered
                .power
                .iter()
                .all(|value| *value == 0 || *value == 255),
            "a binary mode produced a partial power"
        );
    }

    #[test]
    fn floyd_steinberg_preserves_mid_grey_ratio() {
        let luma = [128u8; 16 * 16];
        let bitmap = process_luma(&luma, 16, 16, &settings(ImageMode::FloydSteinberg)).unwrap();
        let burned = bitmap.burned();
        assert!(burned > 80, "too few burned pixels: {burned}");
        assert!(burned < 176, "too many burned pixels: {burned}");
    }

    #[test]
    fn ordered_dither_is_deterministic() {
        let luma = [128u8; 8 * 8];
        let first = process_luma(&luma, 8, 8, &settings(ImageMode::OrderedDither)).unwrap();
        let second = process_luma(&luma, 8, 8, &settings(ImageMode::OrderedDither)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.burned(), 32);
    }

    #[test]
    fn error_diffusion_modes_are_deterministic_and_keep_tone() {
        let luma = [128u8; 16 * 16];
        for mode in [ImageMode::Atkinson, ImageMode::Stucki, ImageMode::Jarvis] {
            let first = process_luma(&luma, 16, 16, &settings(mode)).unwrap();
            let second = process_luma(&luma, 16, 16, &settings(mode)).unwrap();
            assert_eq!(first, second, "{mode:?} is not deterministic");
            let burned = first.burned();
            assert!(
                (60..=196).contains(&burned),
                "{mode:?} burned {burned} of 256 pixels"
            );
        }
    }

    #[test]
    fn halftone_modes_repeat_and_keep_tone() {
        let luma = [128u8; 8 * 8];
        let halftone = process_luma(&luma, 8, 8, &settings(ImageMode::Halftone)).unwrap();
        assert_eq!(halftone.burned(), 32);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(halftone.get(x, y), halftone.get(x + 4, y));
                assert_eq!(halftone.get(x, y), halftone.get(x, y + 4));
            }
        }
        let newsprint = process_luma(&luma, 8, 8, &settings(ImageMode::Newsprint)).unwrap();
        let again = process_luma(&luma, 8, 8, &settings(ImageMode::Newsprint)).unwrap();
        assert_eq!(newsprint, again);
        assert!(newsprint.burned() > 0 && newsprint.burned() < 64);
    }

    #[test]
    fn sketch_burns_only_edges() {
        let flat = [128u8; 8 * 8];
        assert_eq!(
            process_luma(&flat, 8, 8, &settings(ImageMode::Sketch))
                .unwrap()
                .burned(),
            0
        );
        let mut step = vec![0u8; 8 * 8];
        for y in 0..8 {
            for x in 0..8 {
                step[y * 8 + x] = if x < 4 { 0 } else { 255 };
            }
        }
        let edges = process_luma(&step, 8, 8, &settings(ImageMode::Sketch)).unwrap();
        assert!(edges.burned() > 0, "the step edge was not detected");
        assert!(!edges.get(2, 4), "flat interior burned");
        assert!(!edges.get(6, 4), "flat interior burned");
        assert!(!edges.get(0, 4), "border burned");
    }

    #[test]
    fn invalid_buffers_are_rejected() {
        assert!(process_luma(&[], 0, 0, &ImageSettings::default()).is_err());
        assert!(process_luma(&[0, 1], 3, 3, &ImageSettings::default()).is_err());
        let config = ImageSettings {
            contrast: 9.0,
            ..ImageSettings::default()
        };
        assert!(process_luma(&[0], 1, 1, &config).is_err());
    }

    #[test]
    fn missing_file_is_an_error() {
        let path = std::env::temp_dir().join("beamburin_missing_raster.png");
        let _ = std::fs::remove_file(&path);
        assert!(process_file(&path, &ImageSettings::default()).is_err());
    }

    #[test]
    fn processes_a_png_file() {
        let path = std::env::temp_dir().join("beamburin_raster_test.png");
        let mut image = image::GrayImage::new(4, 2);
        for (x, _y, pixel) in image.enumerate_pixels_mut() {
            *pixel = if x < 2 {
                image::Luma([0])
            } else {
                image::Luma([255])
            };
        }
        image.save(&path).unwrap();
        let bitmap = process_file(&path, &ImageSettings::default()).unwrap();
        assert_eq!(bitmap.width, 4);
        assert_eq!(bitmap.height, 2);
        assert_eq!(bitmap.burned(), 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_huge_header_is_rejected_before_decoding() {
        let path = std::env::temp_dir().join("beamburin_huge_header.png");
        write_header_only_png(&path, 60_000, 60_000);
        let error = process_file(&path, &ImageSettings::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("3600000000 pixels"), "{message}");
        assert!(message.contains("limit is"), "{message}");
        let _ = std::fs::remove_file(&path);
    }

    fn write_header_only_png(path: &std::path::Path, width: u32, height: u32) {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for byte in data {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }
            !crc
        }
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&u32::try_from(ihdr.len()).unwrap().to_be_bytes());
        let mut chunk = b"IHDR".to_vec();
        chunk.extend_from_slice(&ihdr);
        png.extend_from_slice(&chunk);
        png.extend_from_slice(&crc32(&chunk).to_be_bytes());
        // An empty IDAT keeps the PNG header parseable while leaving no image
        // data to decode.
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&crc32(b"IDAT").to_be_bytes());
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&crc32(b"IEND").to_be_bytes());
        std::fs::write(path, png).unwrap();
    }
}
