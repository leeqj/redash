use crate::components::theme::DarkTechTheme;
use gpui::*;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SparklineSeries {
    pub label: String,
    pub data: Vec<f32>,
    pub stroke_color: Hsla,
    pub current_val_text: Option<String>,
}

#[allow(dead_code)]
impl SparklineSeries {
    pub fn new(label: impl Into<String>, data: Vec<f32>, stroke_color: Hsla) -> Self {
        Self {
            label: label.into(),
            data,
            stroke_color,
            current_val_text: None,
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.current_val_text = Some(value.into());
        self
    }
}

#[derive(Clone)]
pub struct SparklineChart {
    pub data: Vec<f32>,
    pub series: Vec<SparklineSeries>,
    pub max_value: f32,
    pub stroke_color: Hsla,
    pub fill_color: Hsla,
    pub stroke_width: Pixels,
    pub smooth: bool,
    pub show_pulse_dot: bool,
    pub show_grid: bool,
    pub show_hud_corners: bool,
    pub label: Option<String>,
    pub current_val_text: Option<String>,
    pub show_scale_labels: bool,
    pub show_range: bool,
    pub unit: &'static str,
}

impl SparklineChart {
    pub fn new(data: Vec<f32>, stroke_color: Hsla) -> Self {
        let fill_color = Hsla {
            a: 0.18,
            ..stroke_color
        };
        Self {
            data,
            series: Vec::new(),
            max_value: 100.0,
            stroke_color,
            fill_color,
            stroke_width: px(1.5),
            smooth: true,
            show_pulse_dot: true,
            show_grid: true,
            show_hud_corners: true,
            label: None,
            current_val_text: None,
            show_scale_labels: false,
            show_range: false,
            unit: "%",
        }
    }

    /// Convenience constructor using DarkTech cyan accent color.
    pub fn tech(data: Vec<f32>) -> Self {
        Self::new(data, DarkTechTheme::accent_cyan())
    }

    /// Constructor for multi-series overlaid charts (e.g. CPU + RAM + DISK on one scale).
    pub fn multi(series: Vec<SparklineSeries>) -> Self {
        let stroke_color = series
            .first()
            .map(|s| s.stroke_color)
            .unwrap_or_else(DarkTechTheme::accent_cyan);
        let mut chart = Self::new(Vec::new(), stroke_color);
        chart.series = series;
        chart
    }

    #[allow(dead_code)]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.current_val_text = Some(value.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_scale_labels(mut self, show: bool) -> Self {
        self.show_scale_labels = show;
        self
    }

    #[allow(dead_code)]
    pub fn with_range(mut self, show: bool) -> Self {
        self.show_range = show;
        self
    }

    #[allow(dead_code)]
    pub fn with_unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    #[allow(dead_code)]
    pub fn with_stroke_color(mut self, color: Hsla) -> Self {
        self.stroke_color = color;
        self.fill_color = Hsla { a: 0.18, ..color };
        self
    }

    #[allow(dead_code)]
    pub fn with_max(mut self, max: f32) -> Self {
        self.max_value = max.max(1.0);
        self
    }

    #[allow(dead_code)]
    pub fn with_fill_color(mut self, fill_color: Hsla) -> Self {
        self.fill_color = fill_color;
        self
    }

    #[allow(dead_code)]
    pub fn with_stroke_width(mut self, width: Pixels) -> Self {
        self.stroke_width = width;
        self
    }

    #[allow(dead_code)]
    pub fn with_smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    #[allow(dead_code)]
    pub fn with_pulse_dot(mut self, show: bool) -> Self {
        self.show_pulse_dot = show;
        self
    }

    #[allow(dead_code)]
    pub fn with_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    #[allow(dead_code)]
    pub fn with_hud_corners(mut self, show: bool) -> Self {
        self.show_hud_corners = show;
        self
    }
}

impl IntoElement for SparklineChart {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let max_val = if self.max_value <= 0.0 || self.max_value.is_nan() {
            100.0
        } else {
            self.max_value
        };
        let stroke_col = self.stroke_color;
        let fill_col = self.fill_color;
        let stroke_width = self.stroke_width;
        let smooth = self.smooth;
        let show_pulse_dot = self.show_pulse_dot;
        let show_grid = self.show_grid;
        let show_hud_corners = self.show_hud_corners;
        let label = self.label;
        let current_val_text = self.current_val_text;
        let show_scale_labels = self.show_scale_labels;
        let show_range = self.show_range;
        let unit = self.unit;

        // Build series list
        let series_list: Vec<(Vec<f32>, Hsla, Hsla)> = if !self.series.is_empty() {
            self.series
                .into_iter()
                .map(|s| {
                    let fill = Hsla {
                        a: 0.12,
                        ..s.stroke_color
                    };
                    (s.data, s.stroke_color, fill)
                })
                .collect()
        } else {
            vec![(self.data, stroke_col, fill_col)]
        };

        let has_samples = series_list.iter().any(|(data, _, _)| !data.is_empty());
        // Precompute min and max for range readout
        let (min_val, max_val_samp) = {
            let mut min = 100.0_f32;
            let mut max = 0.0_f32;
            let mut found = false;
            for (d, _, _) in &series_list {
                for &v in d {
                    let v_clean = if v.is_nan() || v < 0.0 { 0.0 } else { v };
                    min = min.min(v_clean);
                    max = max.max(v_clean);
                    found = true;
                }
            }
            if found { (min, max) } else { (0.0, 0.0) }
        };

        let canvas_el = canvas(
            |_bounds, _window, _cx| (),
            move |bounds, (), window, _cx| {
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let origin_x = f32::from(bounds.origin.x);
                let origin_y = f32::from(bounds.origin.y);

                if width <= 0.0 || height <= 0.0 {
                    return;
                }

                // 1. Futuristic HUD Corner Marks: ⌜ ⌝ ⌞ ⌟
                if show_hud_corners {
                    let mark_len = px(3.5);
                    let mark_col = Hsla {
                        a: 0.22,
                        ..DarkTechTheme::border_muted()
                    };
                    let mut corners = PathBuilder::stroke(px(1.0));
                    // Top-left ⌜
                    corners.move_to(point(bounds.origin.x, bounds.origin.y + mark_len));
                    corners.line_to(bounds.origin);
                    corners.line_to(point(bounds.origin.x + mark_len, bounds.origin.y));
                    // Top-right ⌝
                    let tr = point(bounds.origin.x + px(width), bounds.origin.y);
                    corners.move_to(point(tr.x - mark_len, tr.y));
                    corners.line_to(tr);
                    corners.line_to(point(tr.x, tr.y + mark_len));
                    // Bottom-left ⌞
                    let bl = point(bounds.origin.x, bounds.origin.y + px(height));
                    corners.move_to(point(bl.x, bl.y - mark_len));
                    corners.line_to(bl);
                    corners.line_to(point(bl.x + mark_len, bl.y));
                    // Bottom-right ⌟
                    let br = point(bounds.origin.x + px(width), bounds.origin.y + px(height));
                    corners.move_to(point(br.x - mark_len, br.y));
                    corners.line_to(br);
                    corners.line_to(point(br.x, br.y - mark_len));
                    if let Ok(path) = corners.build() {
                        window.paint_path(path, mark_col);
                    }
                }

                let padding_y = 2.0_f32;
                let eff_height = (height - (padding_y * 2.0)).max(1.0);

                // 2. High-Tech Precision Reference Grid Lines (100%, 50%, 0% baseline)
                if show_grid {
                    let grid_col = Hsla {
                        a: 0.12,
                        ..DarkTechTheme::border_muted()
                    };
                    let mut grid = PathBuilder::stroke(px(1.0));
                    let y_100 = origin_y + padding_y;
                    let y_50 = origin_y + padding_y + (eff_height * 0.5);
                    let y_0 = origin_y + padding_y + eff_height;

                    grid.move_to(point(bounds.origin.x, px(y_100)));
                    grid.line_to(point(bounds.origin.x + px(width), px(y_100)));
                    grid.move_to(point(bounds.origin.x, px(y_50)));
                    grid.line_to(point(bounds.origin.x + px(width), px(y_50)));
                    grid.move_to(point(bounds.origin.x, px(y_0)));
                    grid.line_to(point(bounds.origin.x + px(width), px(y_0)));
                    if let Ok(path) = grid.build() {
                        window.paint_path(path, grid_col);
                    }
                }

                let bottom_y = px(origin_y + padding_y + eff_height);

                // 3. Draw each series
                for (s_data, s_stroke_col, s_fill_col) in &series_list {
                    if s_data.is_empty() {
                        continue;
                    }

                    let step = if s_data.len() > 1 {
                        width / (s_data.len() - 1) as f32
                    } else {
                        width
                    };

                    let points: Vec<Point<Pixels>> = s_data
                        .iter()
                        .enumerate()
                        .map(|(i, &val)| {
                            let safe_val = if val.is_nan() || val < 0.0 { 0.0 } else { val };
                            let clamped = safe_val.min(max_val);
                            let norm = clamped / max_val;
                            let x = origin_x + (i as f32 * step);
                            let y = origin_y + padding_y + eff_height - (norm * eff_height);
                            point(px(x), px(y))
                        })
                        .collect();

                    if points.len() >= 2 {
                        // Helper to build curve segments
                        let append_curve = |builder: &mut PathBuilder| {
                            if smooth && points.len() >= 3 {
                                let n = points.len();
                                for i in 0..(n - 1) {
                                    let p0 = if i == 0 { points[0] } else { points[i - 1] };
                                    let p1 = points[i];
                                    let p2 = points[i + 1];
                                    let p3 = if i + 2 < n {
                                        points[i + 2]
                                    } else {
                                        points[n - 1]
                                    };

                                    let cp1 = point(
                                        px(f32::from(p1.x)
                                            + (f32::from(p2.x) - f32::from(p0.x)) / 6.0),
                                        px(f32::from(p1.y)
                                            + (f32::from(p2.y) - f32::from(p0.y)) / 6.0),
                                    );
                                    let cp2 = point(
                                        px(f32::from(p2.x)
                                            - (f32::from(p3.x) - f32::from(p1.x)) / 6.0),
                                        px(f32::from(p2.y)
                                            - (f32::from(p3.y) - f32::from(p1.y)) / 6.0),
                                    );
                                    builder.cubic_bezier_to(p2, cp1, cp2);
                                }
                            } else {
                                for p in &points[1..] {
                                    builder.line_to(*p);
                                }
                            }
                        };

                        // Gradient underfill
                        let mut fill_builder = PathBuilder::fill();
                        fill_builder.move_to(point(bounds.origin.x, bottom_y));
                        fill_builder.line_to(points[0]);
                        append_curve(&mut fill_builder);
                        let last_x = points.last().unwrap().x;
                        fill_builder.line_to(point(last_x, bottom_y));
                        fill_builder.close();
                        if let Ok(fill_path) = fill_builder.build() {
                            window.paint_path(fill_path, *s_fill_col);
                        }

                        // Neon glow stroke
                        let glow_stroke_col = Hsla {
                            a: 0.22,
                            ..*s_stroke_col
                        };
                        let glow_width = px(f32::from(stroke_width) + 1.8);
                        let mut glow_builder = PathBuilder::stroke(glow_width);
                        glow_builder.move_to(points[0]);
                        append_curve(&mut glow_builder);
                        if let Ok(glow_path) = glow_builder.build() {
                            window.paint_path(glow_path, glow_stroke_col);
                        }

                        // Main stroke line
                        let mut stroke_builder = PathBuilder::stroke(stroke_width);
                        stroke_builder.move_to(points[0]);
                        append_curve(&mut stroke_builder);
                        if let Ok(stroke_path) = stroke_builder.build() {
                            window.paint_path(stroke_path, *s_stroke_col);
                        }
                    }

                    // Concentric pulse dot at latest point
                    if show_pulse_dot && let Some(&last_point) = points.last() {
                        let outer_r = px(4.0);
                        let mut outer = fill(
                            Bounds {
                                origin: point(last_point.x - outer_r, last_point.y - outer_r),
                                size: size(outer_r * 2.0, outer_r * 2.0),
                            },
                            Hsla {
                                a: 0.18,
                                ..*s_stroke_col
                            },
                        );
                        outer.corner_radii = (f32::from(outer_r)).into();
                        window.paint_quad(outer);

                        let core_r = px(1.5);
                        let mut core = fill(
                            Bounds {
                                origin: point(last_point.x - core_r, last_point.y - core_r),
                                size: size(core_r * 2.0, core_r * 2.0),
                            },
                            *s_stroke_col,
                        );
                        core.corner_radii = (f32::from(core_r)).into();
                        window.paint_quad(core);
                    }
                }
            },
        )
        .size_full();

        let is_hud =
            show_scale_labels || label.is_some() || current_val_text.is_some() || show_range;

        if !is_hud {
            return div().size_full().child(canvas_el);
        }

        let mut hud = div()
            .flex()
            .flex_col()
            .justify_between()
            .size_full()
            .overflow_hidden();

        // 1. Header (if label or value is provided)
        if label.is_some() || current_val_text.is_some() {
            let header = div()
                .w_full()
                .flex()
                .flex_row()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(div().size(px(4.0)).rounded_full().bg(stroke_col))
                        .children(label.map(|lbl| {
                            div()
                                .text_size(px(9.0))
                                .font_family("Menlo")
                                .font_weight(FontWeight::BOLD)
                                .text_color(stroke_col)
                                .child(lbl)
                        })),
                )
                .children(current_val_text.map(|val| {
                    div()
                        .font_family("Menlo")
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(DarkTechTheme::text_primary())
                        .child(val)
                }));
            hud = hud.child(header);
        }

        // 2. Middle Row: Scale ticks (if enabled) + Canvas
        let mut mid_row = div().flex_1().w_full().min_h(px(0.0)).flex().flex_row();

        if show_scale_labels {
            let scale_col = div()
                .w(px(22.0))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .justify_between()
                .py_0p5()
                .text_size(px(7.5))
                .font_family("Menlo")
                .text_color(DarkTechTheme::text_muted())
                .child(div().child(format!("{:.0}{}", max_val, unit)))
                .child(div().child(format!("{:.0}{}", max_val * 0.5, unit)))
                .child(div().child(format!("0{}", unit)));
            mid_row = mid_row.child(scale_col);
        }

        mid_row = mid_row.child(div().flex_1().h_full().min_w(px(0.0)).child(canvas_el));
        hud = hud.child(mid_row);

        // 3. Footer: MIN / MAX Range (if enabled)
        if show_range && has_samples {
            let footer = div()
                .w_full()
                .flex()
                .flex_row()
                .justify_between()
                .items_center()
                .text_size(px(7.5))
                .font_family("Menlo")
                .text_color(DarkTechTheme::text_muted())
                .child(format!("MIN {:.0}{}", min_val, unit))
                .child(format!("MAX {:.0}{}", max_val_samp, unit));
            hud = hud.child(footer);
        }

        hud
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_sparkline_defaults() {
        let chart = SparklineChart::new(vec![10.0, 20.0, 30.0], rgb(0x38bdf8).into());
        assert_eq!(chart.data.len(), 3);
        assert_eq!(chart.max_value, 100.0);
        assert!(chart.smooth);
        assert!(chart.show_pulse_dot);
        assert!(!chart.show_scale_labels);
        assert!(!chart.show_range);
    }

    #[core::prelude::v1::test]
    fn test_sparkline_customization() {
        let chart = SparklineChart::tech(vec![5.0, 15.0])
            .with_max(50.0)
            .with_smooth(false)
            .with_pulse_dot(false)
            .with_label("CPU")
            .with_value("35.0%")
            .with_scale_labels(true)
            .with_range(true);
        assert_eq!(chart.max_value, 50.0);
        assert!(!chart.smooth);
        assert!(!chart.show_pulse_dot);
        assert_eq!(chart.label.as_deref(), Some("CPU"));
        assert_eq!(chart.current_val_text.as_deref(), Some("35.0%"));
        assert!(chart.show_scale_labels);
        assert!(chart.show_range);
    }

    #[core::prelude::v1::test]
    fn test_sparkline_multi_series() {
        let s1 = SparklineSeries::new("CPU", vec![10.0, 20.0], DarkTechTheme::accent_cyan())
            .with_value("20.0%");
        let s2 = SparklineSeries::new("RAM", vec![30.0, 40.0], DarkTechTheme::accent_indigo())
            .with_value("40.0%");
        let chart = SparklineChart::multi(vec![s1, s2]).with_scale_labels(true);
        assert_eq!(chart.series.len(), 2);
        assert!(chart.show_scale_labels);
    }
}
