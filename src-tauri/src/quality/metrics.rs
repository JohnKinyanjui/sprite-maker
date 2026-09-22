use crate::{
    error::{CommandError, CommandResult},
    AppState,
};
use chrono::Utc;
use image::{imageops::FilterType, RgbaImage};
use rusqlite::{params, OptionalExtension};

pub(super) const ANALYZER_VERSION: &str = "native-v1";

#[derive(Clone)]
pub(crate) struct FrameMetrics {
    pub(crate) asset_id: String,
    pub(crate) content_hash: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bounds: Option<(u32, u32, u32, u32)>,
    pub(crate) centroid: Option<(f64, f64)>,
    pub(crate) alpha_coverage: f64,
    pub(super) opaque_edge_pixels: u32,
    pub(crate) perceptual_hash: u64,
    pub(super) palette: (f64, f64, f64),
}

pub(crate) struct AnalyzedFrame {
    pub(crate) metrics: FrameMetrics,
    pub(crate) image: RgbaImage,
}

pub(super) struct PendingCheck {
    pub(super) check_type: &'static str,
    pub(super) frame_index: Option<u32>,
    pub(super) comparison_frame_index: Option<u32>,
    pub(super) severity: &'static str,
    pub(super) score: f64,
    pub(super) message: String,
    pub(super) metric_value: Option<f64>,
    pub(super) metric_unit: Option<&'static str>,
    pub(super) repair_action: Option<&'static str>,
}

pub(super) fn content_hash(path: &str) -> CommandResult<String> {
    Ok(blake3::hash(&std::fs::read(path)?).to_hex().to_string())
}

pub(crate) fn compute_metrics(asset_id: &str, path: &str) -> CommandResult<AnalyzedFrame> {
    let image = image::open(path)?.to_rgba8();
    let (width, height) = image.dimensions();
    let mut minimum_x = width;
    let mut minimum_y = height;
    let mut maximum_x = 0;
    let mut maximum_y = 0;
    let mut alpha_pixels = 0_u64;
    let mut edge_pixels = 0_u32;
    let mut weighted_x = 0_f64;
    let mut weighted_y = 0_f64;
    let mut alpha_weight = 0_f64;
    let mut red = 0_f64;
    let mut green = 0_f64;
    let mut blue = 0_f64;
    for (x, y, pixel) in image.enumerate_pixels() {
        let alpha = pixel[3];
        if alpha > 8 {
            alpha_pixels += 1;
            minimum_x = minimum_x.min(x);
            minimum_y = minimum_y.min(y);
            maximum_x = maximum_x.max(x);
            maximum_y = maximum_y.max(y);
            if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                edge_pixels += 1;
            }
            let weight = alpha as f64 / 255.0;
            weighted_x += x as f64 * weight;
            weighted_y += y as f64 * weight;
            alpha_weight += weight;
            red += pixel[0] as f64 * weight;
            green += pixel[1] as f64 * weight;
            blue += pixel[2] as f64 * weight;
        }
    }
    let bounds = (alpha_pixels > 0).then_some((minimum_x, minimum_y, maximum_x, maximum_y));
    let centroid =
        (alpha_weight > 0.0).then_some((weighted_x / alpha_weight, weighted_y / alpha_weight));
    let palette = if alpha_weight > 0.0 {
        (
            red / alpha_weight,
            green / alpha_weight,
            blue / alpha_weight,
        )
    } else {
        (0.0, 0.0, 0.0)
    };
    let gray = image::imageops::grayscale(&image);
    let small = image::imageops::resize(&gray, 8, 8, FilterType::Triangle);
    let average = small.pixels().map(|pixel| pixel[0] as u64).sum::<u64>() / 64;
    let mut perceptual_hash = 0_u64;
    for (index, pixel) in small.pixels().enumerate() {
        if pixel[0] as u64 >= average {
            perceptual_hash |= 1_u64 << index;
        }
    }
    Ok(AnalyzedFrame {
        metrics: FrameMetrics {
            asset_id: asset_id.to_string(),
            content_hash: content_hash(path)?,
            width,
            height,
            bounds,
            centroid,
            alpha_coverage: alpha_pixels as f64 / (width as f64 * height as f64).max(1.0),
            opaque_edge_pixels: edge_pixels,
            perceptual_hash,
            palette,
        },
        image,
    })
}

pub(super) fn cache_metrics(state: &AppState, metrics: &FrameMetrics) -> CommandResult<()> {
    let now = Utc::now().to_rfc3339();
    let bounds = metrics.bounds;
    let centroid = metrics.centroid;
    let palette = format!(
        "{:.2},{:.2},{:.2}",
        metrics.palette.0, metrics.palette.1, metrics.palette.2
    );
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO frame_quality_cache(
            asset_id, content_hash, width, height, alpha_min_x, alpha_min_y,
            alpha_max_x, alpha_max_y, centroid_x, centroid_y, alpha_coverage,
            opaque_edge_pixels, perceptual_hash, palette_signature, updated_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
        ON CONFLICT(asset_id) DO UPDATE SET content_hash=excluded.content_hash,
            width=excluded.width, height=excluded.height, alpha_min_x=excluded.alpha_min_x,
            alpha_min_y=excluded.alpha_min_y, alpha_max_x=excluded.alpha_max_x,
            alpha_max_y=excluded.alpha_max_y, centroid_x=excluded.centroid_x,
            centroid_y=excluded.centroid_y, alpha_coverage=excluded.alpha_coverage,
            opaque_edge_pixels=excluded.opaque_edge_pixels,
            perceptual_hash=excluded.perceptual_hash,
            palette_signature=excluded.palette_signature, updated_at=excluded.updated_at"#,
        params![
            metrics.asset_id,
            metrics.content_hash,
            metrics.width,
            metrics.height,
            bounds.map(|value| value.0),
            bounds.map(|value| value.1),
            bounds.map(|value| value.2),
            bounds.map(|value| value.3),
            centroid.map(|value| value.0),
            centroid.map(|value| value.1),
            metrics.alpha_coverage,
            metrics.opaque_edge_pixels,
            format!("{:016x}", metrics.perceptual_hash),
            palette,
            now
        ],
    )?;
    Ok(())
}

pub(super) fn load_cached_metrics(
    state: &AppState,
    asset_id: &str,
    expected_hash: &str,
) -> CommandResult<Option<FrameMetrics>> {
    type CachedFrame = (
        String,
        u32,
        u32,
        Option<u32>,
        Option<u32>,
        Option<u32>,
        Option<u32>,
        Option<f64>,
        Option<f64>,
        f64,
        u32,
        String,
        String,
    );
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let cached: Option<CachedFrame> = connection
        .query_row(
            r#"SELECT content_hash,width,height,alpha_min_x,alpha_min_y,alpha_max_x,
                      alpha_max_y,centroid_x,centroid_y,alpha_coverage,
                      opaque_edge_pixels,perceptual_hash,palette_signature
               FROM frame_quality_cache WHERE asset_id=?1"#,
            [asset_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()?;
    let Some((
        hash,
        width,
        height,
        min_x,
        min_y,
        max_x,
        max_y,
        centroid_x,
        centroid_y,
        coverage,
        edge_pixels,
        perceptual,
        palette,
    )) = cached
    else {
        return Ok(None);
    };
    if hash != expected_hash {
        return Ok(None);
    }
    let palette: Vec<_> = palette
        .split(',')
        .filter_map(|value| value.parse::<f64>().ok())
        .collect();
    let bounds = match (min_x, min_y, max_x, max_y) {
        (Some(minimum_x), Some(minimum_y), Some(maximum_x), Some(maximum_y)) => {
            Some((minimum_x, minimum_y, maximum_x, maximum_y))
        }
        _ => None,
    };
    let centroid = centroid_x.zip(centroid_y);
    Ok(Some(FrameMetrics {
        asset_id: asset_id.to_string(),
        content_hash: hash,
        width,
        height,
        bounds,
        centroid,
        alpha_coverage: coverage,
        opaque_edge_pixels: edge_pixels,
        perceptual_hash: u64::from_str_radix(&perceptual, 16).unwrap_or_default(),
        palette: (
            palette.first().copied().unwrap_or_default(),
            palette.get(1).copied().unwrap_or_default(),
            palette.get(2).copied().unwrap_or_default(),
        ),
    }))
}

pub(crate) fn pixel_difference(first: &RgbaImage, second: &RgbaImage) -> f64 {
    let width = first.width().max(second.width()).max(1);
    let height = first.height().max(second.height()).max(1);
    let first = image::imageops::resize(first, width, height, FilterType::Nearest);
    let second = image::imageops::resize(second, width, height, FilterType::Nearest);
    let total = first
        .pixels()
        .zip(second.pixels())
        .map(|(left, right)| {
            (0..4)
                .map(|channel| (left[channel] as f64 - right[channel] as f64).abs())
                .sum::<f64>()
        })
        .sum::<f64>();
    total / (width as f64 * height as f64 * 4.0 * 255.0)
}

pub(super) fn palette_distance(first: &FrameMetrics, second: &FrameMetrics) -> f64 {
    ((first.palette.0 - second.palette.0).powi(2)
        + (first.palette.1 - second.palette.1).powi(2)
        + (first.palette.2 - second.palette.2).powi(2))
    .sqrt()
}

pub(crate) fn centroid_distance(first: &FrameMetrics, second: &FrameMetrics) -> f64 {
    match (first.centroid, second.centroid) {
        (Some(first), Some(second)) => {
            ((first.0 - second.0).powi(2) + (first.1 - second.1).powi(2)).sqrt()
        }
        _ => 0.0,
    }
}

pub(crate) fn bounds_area(metrics: &FrameMetrics) -> f64 {
    metrics
        .bounds
        .map(|(minimum_x, minimum_y, maximum_x, maximum_y)| {
            (maximum_x - minimum_x + 1) as f64 * (maximum_y - minimum_y + 1) as f64
        })
        .unwrap_or(0.0)
}

pub(super) fn score_with_penalty(penalty: f64) -> f64 {
    (100.0 - penalty).clamp(0.0, 100.0)
}
