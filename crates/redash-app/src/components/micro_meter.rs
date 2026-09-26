use crate::components::theme::DarkTechTheme;
use gpui::*;

#[allow(dead_code)]
#[derive(Clone)]
pub struct MicroMeter {
    pub label: String,
    pub value_str: String,
    pub percentage: f32, // 0.0 - 100.0
    pub bar_color: Hsla,
    pub height: Pixels,
}

#[allow(dead_code)]
impl MicroMeter {
    pub fn new(
        label: impl Into<String>,
        value_str: impl Into<String>,
        percentage: f32,
        bar_color: Hsla,
    ) -> Self {
        Self {
            label: label.into(),
            value_str: value_str.into(),
            percentage: percentage.clamp(0.0, 100.0),
            bar_color,
            height: px(4.0),
        }
    }

    pub fn with_height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    /// Helper returning appropriate color according to percentage threshold.
    pub fn color_for_percentage(pct: f32) -> Hsla {
        if pct > 85.0 {
            DarkTechTheme::status_crit()
        } else if pct > 70.0 {
            DarkTechTheme::status_warn()
        } else {
            DarkTechTheme::accent_cyan()
        }
    }

    /// Convenience constructor for CPU telemetry bar.
    pub fn cpu(percentage: f32) -> Self {
        let pct = percentage.clamp(0.0, 100.0);
        let color = Self::color_for_percentage(pct);
        Self::new("CPU", format!("{:.1}%", pct), pct, color)
    }

    /// Convenience constructor for RAM telemetry bar.
    pub fn memory(used_gb: f32, total_gb: f32, percentage: f32) -> Self {
        let pct = percentage.clamp(0.0, 100.0);
        let color = Self::color_for_percentage(pct);
        Self::new(
            "RAM",
            format!("{:.1}G/{:.1}G", used_gb, total_gb),
            pct,
            color,
        )
    }

    /// Convenience constructor for Disk telemetry bar.
    pub fn disk(percentage: f32) -> Self {
        let pct = percentage.clamp(0.0, 100.0);
        let color = Self::color_for_percentage(pct);
        Self::new("DISK", format!("{:.0}%", pct), pct, color)
    }
}

impl RenderOnce for MicroMeter {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let pct = self.percentage;
        let fill_color = self.bar_color;
        let meter_height = self.height;
        let corner_r = f32::from(meter_height) * 0.5;

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .text_size(px(10.0))
                    .font_family("Menlo")
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .child(
                                // Micro status dot
                                div().size(px(4.0)).rounded_full().bg(fill_color),
                            )
                            .child(
                                div()
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(self.label),
                            ),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(DarkTechTheme::bg_input())
                            .border_1()
                            .border_color(DarkTechTheme::border_muted())
                            .text_color(if pct > 85.0 {
                                DarkTechTheme::status_crit()
                            } else if pct > 70.0 {
                                DarkTechTheme::status_warn()
                            } else {
                                DarkTechTheme::text_primary()
                            })
                            .font_weight(FontWeight::BOLD)
                            .child(self.value_str),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(meter_height)
                    .rounded_sm()
                    .bg(DarkTechTheme::bg_input())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .overflow_hidden()
                    .child(
                        canvas(
                            move |_bounds, _window, _cx| (),
                            move |bounds, (), window, _cx| {
                                let total_w = f32::from(bounds.size.width);
                                let total_h = f32::from(bounds.size.height);

                                // 1. HUD segmented tick marks at 25%, 50%, 75%
                                let tick_col = Hsla {
                                    a: 0.18,
                                    ..DarkTechTheme::border_muted()
                                };
                                for tick in [0.25, 0.50, 0.75] {
                                    let tx = bounds.origin.x + px(total_w * tick);
                                    let mut p = PathBuilder::stroke(px(1.0));
                                    p.move_to(point(tx, bounds.origin.y));
                                    p.line_to(point(tx, bounds.origin.y + px(total_h)));
                                    if let Ok(path) = p.build() {
                                        window.paint_path(path, tick_col);
                                    }
                                }

                                // 2. Main Luminous Progress Fill
                                let width = total_w * (pct / 100.0);
                                if width > 0.0 {
                                    let fill_bounds = Bounds {
                                        origin: bounds.origin,
                                        size: size(px(width), bounds.size.height),
                                    };
                                    let mut quad = fill(fill_bounds, fill_color);
                                    quad.corner_radii = (corner_r).into();
                                    window.paint_quad(quad);

                                    // 3. Leading Head Glow Cap
                                    let cap_w = px(2.5_f32.min(width));
                                    let cap_bounds = Bounds {
                                        origin: point(
                                            bounds.origin.x + px(width) - cap_w,
                                            bounds.origin.y,
                                        ),
                                        size: size(cap_w, bounds.size.height),
                                    };
                                    let mut cap_quad = fill(
                                        cap_bounds,
                                        Hsla {
                                            l: (fill_color.l * 1.3).min(1.0),
                                            a: 0.95,
                                            ..fill_color
                                        },
                                    );
                                    cap_quad.corner_radii = (corner_r).into();
                                    window.paint_quad(cap_quad);
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_micro_meter_clamping() {
        let meter_low = MicroMeter::new("Test", "0%", -15.0, DarkTechTheme::accent_cyan());
        assert_eq!(meter_low.percentage, 0.0);

        let meter_high = MicroMeter::new("Test", "120%", 120.0, DarkTechTheme::accent_cyan());
        assert_eq!(meter_high.percentage, 100.0);
    }

    #[core::prelude::v1::test]
    fn test_micro_meter_color_for_percentage() {
        assert_eq!(
            MicroMeter::color_for_percentage(50.0),
            DarkTechTheme::accent_cyan()
        );
        assert_eq!(
            MicroMeter::color_for_percentage(75.0),
            DarkTechTheme::status_warn()
        );
        assert_eq!(
            MicroMeter::color_for_percentage(92.0),
            DarkTechTheme::status_crit()
        );
    }

    #[core::prelude::v1::test]
    fn test_micro_meter_convenience_constructors() {
        let cpu_meter = MicroMeter::cpu(55.4);
        assert_eq!(cpu_meter.label, "CPU");
        assert_eq!(cpu_meter.value_str, "55.4%");

        let ram_meter = MicroMeter::memory(4.2, 16.0, 26.25);
        assert_eq!(ram_meter.label, "RAM");
        assert_eq!(ram_meter.value_str, "4.2G/16.0G");

        let disk_meter = MicroMeter::disk(82.0);
        assert_eq!(disk_meter.label, "DISK");
        assert_eq!(disk_meter.value_str, "82%");
    }
}
