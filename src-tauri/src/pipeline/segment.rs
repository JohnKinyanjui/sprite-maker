use super::qc::vertical_alpha_profile;
use image::RgbaImage;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrameCountInference {
    pub suggested_frame_count: u32,
    pub confidence: f64,
    pub gutter_positions: Vec<u32>,
}

fn merge_nearby_positions(positions: Vec<u32>, min_gap: u32) -> Vec<u32> {
    if positions.is_empty() {
        return positions;
    }
    let mut merged = Vec::new();
    let mut current = positions[0];
    for position in positions.into_iter().skip(1) {
        if position - current <= min_gap {
            continue;
        }
        merged.push(current);
        current = position;
    }
    merged.push(current);
    merged
}

fn segment_width_confidence(widths: &[u32]) -> f64 {
    if widths.is_empty() {
        return 0.0;
    }
    let mean = widths.iter().map(|value| *value as f64).sum::<f64>() / widths.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let variance = widths
        .iter()
        .map(|value| {
            let delta = *value as f64 - mean;
            delta * delta
        })
        .sum::<f64>()
        / widths.len() as f64;
    let coefficient = variance.sqrt() / mean;
    (1.0 - coefficient).clamp(0.0, 1.0)
}

pub(crate) fn boundaries_from_gutters(width: u32, gutters: &[u32]) -> Vec<u32> {
    let mut boundaries = vec![0_u32];
    boundaries.extend(gutters);
    boundaries.push(width);
    boundaries.sort();
    boundaries.dedup();
    boundaries
}

pub(crate) fn segments_from_boundaries(boundaries: &[u32]) -> Vec<(u32, u32)> {
    let mut segments = Vec::new();
    for index in 0..boundaries.len().saturating_sub(1) {
        let start = boundaries[index];
        let end = boundaries[index + 1];
        if end > start {
            segments.push((start, end - start));
        }
    }
    segments
}

pub(crate) fn infer_frame_count(width: u32, profile: &[u32], height: u32) -> FrameCountInference {
    if width < 8 || height == 0 {
        return FrameCountInference {
            suggested_frame_count: 1,
            confidence: 0.0,
            gutter_positions: Vec::new(),
        };
    }

    let max_value = profile.iter().copied().max().unwrap_or(1).max(1);
    let threshold = max_value / 4;

    let mut valleys = Vec::new();
    for x in 2..profile.len().saturating_sub(2) {
        let value = profile[x];
        if value <= threshold && value < profile[x - 1] && value <= profile[x + 1] {
            valleys.push((x as u32, value));
        }
    }

    let min_cell_width = 6_u32;
    let max_frames = (width / min_cell_width).max(1).min(64) as usize;
    valleys.sort_by_key(|(_, value)| *value);
    if valleys.len() + 1 > max_frames {
        valleys.truncate(max_frames.saturating_sub(1));
    }
    let mut gutters = valleys.into_iter().map(|(x, _)| x).collect::<Vec<_>>();
    gutters.sort();
    gutters = merge_nearby_positions(gutters, 4);

    let suggested_frame_count = gutters.len().saturating_add(1).clamp(1, 64) as u32;
    let boundaries = boundaries_from_gutters(width, &gutters);
    let widths = segments_from_boundaries(&boundaries)
        .into_iter()
        .map(|(_, width)| width)
        .collect::<Vec<_>>();
    let confidence = segment_width_confidence(&widths);

    FrameCountInference {
        suggested_frame_count,
        confidence,
        gutter_positions: gutters,
    }
}

fn gutter_cut_cost(profile: &[u32], cut: usize) -> u32 {
    if profile.is_empty() {
        return 0;
    }
    let left = cut.saturating_sub(1);
    let center = cut.min(profile.len() - 1);
    let right = (cut + 1).min(profile.len() - 1);
    (profile[left] + profile[center] + profile[right]) / 3
}

pub(crate) fn dp_horizontal_segments(
    width: u32,
    profile: &[u32],
    frame_count: u32,
) -> Vec<(u32, u32)> {
    let frame_count = frame_count.max(1) as usize;
    let width = width as usize;
    if frame_count == 1 {
        return vec![(0, width as u32)];
    }
    if width < frame_count {
        return Vec::new();
    }

    const MIN_SEGMENT_WIDTH: usize = 4;
    const INF: u32 = u32::MAX / 4;
    let mut dp = vec![vec![INF; width + 1]; frame_count + 1];
    dp[0][0] = 0;

    for segments in 1..=frame_count {
        let min_end = segments * MIN_SEGMENT_WIDTH;
        for end in min_end..=width {
            let min_prev = (segments - 1) * MIN_SEGMENT_WIDTH;
            for prev in min_prev..end {
                let segment_width = end - prev;
                if segment_width < MIN_SEGMENT_WIDTH {
                    continue;
                }
                let cut_cost = if prev > 0 {
                    gutter_cut_cost(profile, prev)
                } else {
                    0
                };
                let candidate = dp[segments - 1][prev].saturating_add(cut_cost);
                if candidate < dp[segments][end] {
                    dp[segments][end] = candidate;
                }
            }
        }
    }

    if dp[frame_count][width] == INF {
        return Vec::new();
    }

    let mut boundaries = vec![width];
    let mut segments_left = frame_count;
    let mut end = width;
    while segments_left > 1 {
        let target = dp[segments_left][end];
        let min_prev = (segments_left - 1) * MIN_SEGMENT_WIDTH;
        let matching = (min_prev..end).filter(|prev| {
            if end - *prev < MIN_SEGMENT_WIDTH {
                return false;
            }
            let cut_cost = if *prev > 0 {
                gutter_cut_cost(profile, *prev)
            } else {
                0
            };
            dp[segments_left - 1][*prev].saturating_add(cut_cost) == target
        });
        let chosen = matching
            .max()
            .unwrap_or_else(|| {
                (min_prev..end)
                    .filter(|prev| end - *prev >= MIN_SEGMENT_WIDTH)
                    .min_by_key(|prev| {
                        let cut_cost = if *prev > 0 {
                            gutter_cut_cost(profile, *prev)
                        } else {
                            0
                        };
                        dp[segments_left - 1][*prev].saturating_add(cut_cost)
                    })
                    .unwrap_or(min_prev)
            });
        boundaries.push(chosen);
        end = chosen;
        segments_left -= 1;
    }
    boundaries.push(0);
    boundaries.sort();
    boundaries.dedup();

    segments_from_boundaries(
        &boundaries
            .into_iter()
            .map(|value| value as u32)
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn infer_frame_count_from_image(image: &RgbaImage) -> FrameCountInference {
    let (width, height) = image.dimensions();
    let profile = vertical_alpha_profile(image);
    infer_frame_count(width, &profile, height)
}

pub(crate) fn equal_horizontal_segments(width: u32, frame_count: u32) -> Vec<(u32, u32)> {
    let frame_count = frame_count.max(1);
    let cell_width = width / frame_count;
    (0..frame_count)
        .map(|index| (index * cell_width, cell_width))
        .collect()
}

pub(crate) fn equal_vertical_segment_heights(height: u32, frame_count: u32) -> Vec<u32> {
    let frame_count = frame_count.max(1);
    let cell_height = height / frame_count;
    (0..frame_count).map(|_| cell_height).collect()
}

pub(crate) fn segments_cover_width(segments: &[(u32, u32)], width: u32) -> bool {
    if segments.is_empty() {
        return false;
    }
    let total = segments.iter().map(|(_, segment_width)| *segment_width).sum::<u32>();
    total == width && segments.iter().all(|(_, segment_width)| *segment_width >= 4)
}
