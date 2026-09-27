use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeterLevel {
    Normal,
    Warning,
    Critical,
}

/// Evaluates health level based on load percentage (0.0 to 100.0).
pub fn evaluate_meter_level(pct: f32) -> MeterLevel {
    if pct >= 90.0 {
        MeterLevel::Critical
    } else if pct >= 75.0 {
        MeterLevel::Warning
    } else {
        MeterLevel::Normal
    }
}

/// Normalizes raw telemetry samples into 2D coordinate points for Sparkline charts.
pub fn normalize_sparkline(
    samples: &[f32],
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
) -> Vec<(f64, f64)> {
    if samples.is_empty() {
        return Vec::new();
    }
    if samples.len() == 1 {
        return vec![(origin_x + width / 2.0, origin_y + height / 2.0)];
    }

    let step_x = width / (samples.len() - 1) as f64;
    samples
        .iter()
        .enumerate()
        .map(|(idx, &val)| {
            let clamped = val.clamp(0.0, 100.0) as f64;
            let norm_y = 1.0 - (clamped / 100.0);
            let px = origin_x + (idx as f64) * step_x;
            let py = origin_y + norm_y * height;
            (px, py)
        })
        .collect()
}
