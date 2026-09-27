use crate::components::theme::DarkTechTheme;
use gpui::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    // Navigation & Primary
    Server,
    Terminal,
    Settings,
    Zap,
    Folder,
    File,
    Search,
    Plus,
    Close,
    Check,

    // Settings Categories
    Activity,
    Palette,
    Bell,
    Shield,
    Info,

    // Actions & Operations
    Play,
    Stop,
    Refresh,
    Trash,
    Edit,
    Copy,
    Save,
    Undo,
    Clear,
    ArrowUp,
    ArrowDown,
    Sort,
    Grip,

    // Systems & Diagnostics
    Docker,
    Cpu,
    #[allow(dead_code)]
    Memory,
    #[allow(dead_code)]
    Disk,
    Network,
    Tunnel,
    Snippet,

    // Target OS
    Linux,
    Apple,
    Windows,

    // File, Transfer & Time
    Link,
    Archive,
    Download,
    Upload,
    Clock,
}

#[derive(Clone)]
pub struct Icon {
    pub icon_type: IconType,
    pub size: Pixels,
    pub color: Option<Hsla>,
}

impl Icon {
    pub fn new(icon_type: IconType) -> Self {
        Self {
            icon_type,
            size: px(14.0),
            color: None,
        }
    }

    pub fn with_size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn with_color<C: Into<Hsla>>(mut self, color: C) -> Self {
        self.color = Some(color.into());
        self
    }

    // Convenience constructors
    pub fn server() -> Self {
        Self::new(IconType::Server)
    }
    pub fn terminal() -> Self {
        Self::new(IconType::Terminal)
    }
    pub fn settings() -> Self {
        Self::new(IconType::Settings)
    }
    pub fn zap() -> Self {
        Self::new(IconType::Zap)
    }
    pub fn folder() -> Self {
        Self::new(IconType::Folder)
    }
    pub fn file() -> Self {
        Self::new(IconType::File)
    }
    pub fn search() -> Self {
        Self::new(IconType::Search)
    }
    pub fn plus() -> Self {
        Self::new(IconType::Plus)
    }
    pub fn close() -> Self {
        Self::new(IconType::Close)
    }
    pub fn check() -> Self {
        Self::new(IconType::Check)
    }
    pub fn activity() -> Self {
        Self::new(IconType::Activity)
    }
    pub fn palette() -> Self {
        Self::new(IconType::Palette)
    }
    pub fn bell() -> Self {
        Self::new(IconType::Bell)
    }
    pub fn shield() -> Self {
        Self::new(IconType::Shield)
    }
    pub fn info() -> Self {
        Self::new(IconType::Info)
    }
    pub fn play() -> Self {
        Self::new(IconType::Play)
    }
    pub fn stop() -> Self {
        Self::new(IconType::Stop)
    }
    pub fn refresh() -> Self {
        Self::new(IconType::Refresh)
    }
    pub fn trash() -> Self {
        Self::new(IconType::Trash)
    }
    pub fn edit() -> Self {
        Self::new(IconType::Edit)
    }
    pub fn copy() -> Self {
        Self::new(IconType::Copy)
    }
    pub fn save() -> Self {
        Self::new(IconType::Save)
    }
    pub fn undo() -> Self {
        Self::new(IconType::Undo)
    }
    pub fn clear() -> Self {
        Self::new(IconType::Clear)
    }
    pub fn arrow_up() -> Self {
        Self::new(IconType::ArrowUp)
    }
    pub fn arrow_down() -> Self {
        Self::new(IconType::ArrowDown)
    }
    pub fn sort() -> Self {
        Self::new(IconType::Sort)
    }
    pub fn grip() -> Self {
        Self::new(IconType::Grip)
    }
    pub fn docker() -> Self {
        Self::new(IconType::Docker)
    }
    pub fn cpu() -> Self {
        Self::new(IconType::Cpu)
    }
    #[allow(dead_code)]
    pub fn memory() -> Self {
        Self::new(IconType::Memory)
    }
    #[allow(dead_code)]
    pub fn disk() -> Self {
        Self::new(IconType::Disk)
    }
    pub fn network() -> Self {
        Self::new(IconType::Network)
    }
    pub fn tunnel() -> Self {
        Self::new(IconType::Tunnel)
    }
    pub fn snippet() -> Self {
        Self::new(IconType::Snippet)
    }
    pub fn linux() -> Self {
        Self::new(IconType::Linux)
    }
    pub fn apple() -> Self {
        Self::new(IconType::Apple)
    }
    pub fn windows() -> Self {
        Self::new(IconType::Windows)
    }
    pub fn link() -> Self {
        Self::new(IconType::Link)
    }
    pub fn archive() -> Self {
        Self::new(IconType::Archive)
    }
    pub fn download() -> Self {
        Self::new(IconType::Download)
    }
    pub fn upload() -> Self {
        Self::new(IconType::Upload)
    }
    pub fn clock() -> Self {
        Self::new(IconType::Clock)
    }
}

impl IntoElement for Icon {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let icon_type = self.icon_type;
        let icon_size = self.size;
        let icon_color = self.color.unwrap_or_else(DarkTechTheme::text_secondary);

        div().size(icon_size).flex_shrink_0().child(
            canvas(
                move |_bounds, _window, _cx| (),
                move |bounds, (), window, _cx| {
                    let w = f32::from(bounds.size.width);
                    let h = f32::from(bounds.size.height);
                    if w <= 0.0 || h <= 0.0 {
                        return;
                    }

                    let ox = f32::from(bounds.origin.x);
                    let oy = f32::from(bounds.origin.y);

                    // Helper to map normalized coordinates (0.0..=1.0) to actual canvas pixels
                    let pt = |nx: f32, ny: f32| point(px(ox + nx * w), px(oy + ny * h));
                    let stroke_w = px((1.3 * (w / 14.0)).max(1.0));

                    match icon_type {
                        IconType::Server => {
                            // Top rack box
                            let mut p1 = PathBuilder::stroke(stroke_w);
                            p1.move_to(pt(0.12, 0.20));
                            p1.line_to(pt(0.88, 0.20));
                            p1.line_to(pt(0.88, 0.45));
                            p1.line_to(pt(0.12, 0.45));
                            p1.close();
                            if let Ok(path) = p1.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Bottom rack box
                            let mut p2 = PathBuilder::stroke(stroke_w);
                            p2.move_to(pt(0.12, 0.55));
                            p2.line_to(pt(0.88, 0.55));
                            p2.line_to(pt(0.88, 0.80));
                            p2.line_to(pt(0.12, 0.80));
                            p2.close();
                            if let Ok(path) = p2.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Rack LEDs
                            let r = px(w * 0.06);
                            let q1 = fill(
                                Bounds {
                                    origin: pt(0.24, 0.325),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            let q2 = fill(
                                Bounds {
                                    origin: pt(0.24, 0.675),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            window.paint_quad(q1);
                            window.paint_quad(q2);
                        }

                        IconType::Terminal => {
                            // Chevron prompt: >
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.28));
                            p.line_to(pt(0.48, 0.50));
                            p.line_to(pt(0.18, 0.72));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Cursor underscore: _
                            let mut p2 = PathBuilder::stroke(stroke_w);
                            p2.move_to(pt(0.56, 0.72));
                            p2.line_to(pt(0.84, 0.72));
                            if let Ok(path) = p2.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Settings => {
                            // Modern cog / sliders: Central gear ring + axis ticks
                            let _r_out = w * 0.36;
                            let _r_in = w * 0.18;
                            let center_pt = pt(0.5, 0.5);

                            let mut ring = PathBuilder::stroke(stroke_w);
                            // Approximate circular hub
                            let steps = 8;
                            for i in 0..steps {
                                let angle =
                                    (i as f32) * std::f32::consts::PI * 2.0 / (steps as f32);
                                let px_pos = pt(0.5 + angle.cos() * 0.26, 0.5 + angle.sin() * 0.26);
                                if i == 0 {
                                    ring.move_to(px_pos);
                                } else {
                                    ring.line_to(px_pos);
                                }
                            }
                            ring.close();
                            if let Ok(path) = ring.build() {
                                window.paint_path(path, icon_color);
                            }

                            // 4 Outer teeth
                            let mut teeth = PathBuilder::stroke(stroke_w);
                            teeth.move_to(pt(0.5, 0.08));
                            teeth.line_to(pt(0.5, 0.22));
                            teeth.move_to(pt(0.5, 0.78));
                            teeth.line_to(pt(0.5, 0.92));
                            teeth.move_to(pt(0.08, 0.5));
                            teeth.line_to(pt(0.22, 0.5));
                            teeth.move_to(pt(0.78, 0.5));
                            teeth.line_to(pt(0.92, 0.5));
                            if let Ok(path) = teeth.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Center solid core
                            let core_r = px(w * 0.08);
                            let core = fill(
                                Bounds {
                                    origin: point(center_pt.x - core_r, center_pt.y - core_r),
                                    size: size(core_r * 2.0, core_r * 2.0),
                                },
                                icon_color,
                            );
                            window.paint_quad(core);
                        }

                        IconType::Zap => {
                            // High-tech angular lightning bolt
                            let mut p = PathBuilder::fill();
                            p.move_to(pt(0.58, 0.10));
                            p.line_to(pt(0.22, 0.52));
                            p.line_to(pt(0.48, 0.52));
                            p.line_to(pt(0.40, 0.92));
                            p.line_to(pt(0.78, 0.46));
                            p.line_to(pt(0.52, 0.46));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Folder => {
                            // Sleek tech folder outline
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.12, 0.28));
                            p.line_to(pt(0.40, 0.28));
                            p.line_to(pt(0.50, 0.38));
                            p.line_to(pt(0.88, 0.38));
                            p.line_to(pt(0.88, 0.78));
                            p.line_to(pt(0.12, 0.78));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::File => {
                            // Document page with dog-eared top-right corner
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.20, 0.15));
                            p.line_to(pt(0.60, 0.15));
                            p.line_to(pt(0.80, 0.35));
                            p.line_to(pt(0.80, 0.85));
                            p.line_to(pt(0.20, 0.85));
                            p.close();
                            // Fold corner
                            p.move_to(pt(0.60, 0.15));
                            p.line_to(pt(0.60, 0.35));
                            p.line_to(pt(0.80, 0.35));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Search => {
                            // Magnifying glass
                            let mut circle = PathBuilder::stroke(stroke_w);
                            let steps = 10;
                            for i in 0..steps {
                                let a = (i as f32) * std::f32::consts::PI * 2.0 / (steps as f32);
                                let pos = pt(0.42 + a.cos() * 0.26, 0.42 + a.sin() * 0.26);
                                if i == 0 {
                                    circle.move_to(pos);
                                } else {
                                    circle.line_to(pos);
                                }
                            }
                            circle.close();
                            if let Ok(path) = circle.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Angled handle
                            let mut handle = PathBuilder::stroke(stroke_w);
                            handle.move_to(pt(0.60, 0.60));
                            handle.line_to(pt(0.86, 0.86));
                            if let Ok(path) = handle.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Plus => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.5, 0.2));
                            p.line_to(pt(0.5, 0.8));
                            p.move_to(pt(0.2, 0.5));
                            p.line_to(pt(0.8, 0.5));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Close => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.25, 0.25));
                            p.line_to(pt(0.75, 0.75));
                            p.move_to(pt(0.75, 0.25));
                            p.line_to(pt(0.25, 0.75));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Check => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.20, 0.52));
                            p.line_to(pt(0.42, 0.74));
                            p.line_to(pt(0.82, 0.26));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Activity => {
                            // Telemetry pulse waveform
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.10, 0.50));
                            p.line_to(pt(0.32, 0.50));
                            p.line_to(pt(0.44, 0.20));
                            p.line_to(pt(0.56, 0.80));
                            p.line_to(pt(0.68, 0.50));
                            p.line_to(pt(0.90, 0.50));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Palette => {
                            // 4 color swatch quadrants
                            let pad = 0.08;
                            let mid = 0.5;
                            let r = px(w * 0.05);

                            let b1 = Bounds {
                                origin: pt(pad, pad),
                                size: size(px((mid - pad * 1.5) * w), px((mid - pad * 1.5) * h)),
                            };
                            let b2 = Bounds {
                                origin: pt(mid + pad * 0.5, pad),
                                size: size(px((mid - pad * 1.5) * w), px((mid - pad * 1.5) * h)),
                            };
                            let b3 = Bounds {
                                origin: pt(pad, mid + pad * 0.5),
                                size: size(px((mid - pad * 1.5) * w), px((mid - pad * 1.5) * h)),
                            };
                            let b4 = Bounds {
                                origin: pt(mid + pad * 0.5, mid + pad * 0.5),
                                size: size(px((mid - pad * 1.5) * w), px((mid - pad * 1.5) * h)),
                            };

                            let mut q1 = fill(b1, icon_color);
                            q1.corner_radii = r.into();
                            let mut q2 = fill(
                                b2,
                                Hsla {
                                    a: icon_color.a * 0.7,
                                    ..icon_color
                                },
                            );
                            q2.corner_radii = r.into();
                            let mut q3 = fill(
                                b3,
                                Hsla {
                                    a: icon_color.a * 0.5,
                                    ..icon_color
                                },
                            );
                            q3.corner_radii = r.into();
                            let mut q4 = fill(
                                b4,
                                Hsla {
                                    a: icon_color.a * 0.3,
                                    ..icon_color
                                },
                            );
                            q4.corner_radii = r.into();

                            window.paint_quad(q1);
                            window.paint_quad(q2);
                            window.paint_quad(q3);
                            window.paint_quad(q4);
                        }

                        IconType::Bell => {
                            // Sleek notification bell
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.5, 0.15));
                            p.line_to(pt(0.5, 0.22));
                            p.move_to(pt(0.30, 0.65));
                            p.line_to(pt(0.30, 0.42));
                            p.cubic_bezier_to(pt(0.70, 0.42), pt(0.30, 0.22), pt(0.70, 0.22));
                            p.line_to(pt(0.70, 0.65));
                            p.line_to(pt(0.20, 0.65));
                            p.line_to(pt(0.80, 0.65));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Clapper bottom dot
                            let r = px(w * 0.08);
                            let clapper = fill(
                                Bounds {
                                    origin: pt(0.44, 0.74),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            window.paint_quad(clapper);
                        }

                        IconType::Shield => {
                            // Tech security shield crest
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.20));
                            p.line_to(pt(0.82, 0.20));
                            p.line_to(pt(0.82, 0.50));
                            p.cubic_bezier_to(pt(0.50, 0.88), pt(0.82, 0.72), pt(0.65, 0.85));
                            p.cubic_bezier_to(pt(0.18, 0.50), pt(0.35, 0.85), pt(0.18, 0.72));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Info => {
                            // Circular info badge
                            let mut circle = PathBuilder::stroke(stroke_w);
                            let steps = 10;
                            for i in 0..steps {
                                let a = (i as f32) * std::f32::consts::PI * 2.0 / (steps as f32);
                                let pos = pt(0.5 + a.cos() * 0.36, 0.5 + a.sin() * 0.36);
                                if i == 0 {
                                    circle.move_to(pos);
                                } else {
                                    circle.line_to(pos);
                                }
                            }
                            circle.close();
                            if let Ok(path) = circle.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Dot and stem
                            let dot = fill(
                                Bounds {
                                    origin: pt(0.45, 0.28),
                                    size: size(px(w * 0.1), px(w * 0.1)),
                                },
                                icon_color,
                            );
                            window.paint_quad(dot);
                            let mut stem = PathBuilder::stroke(stroke_w);
                            stem.move_to(pt(0.50, 0.44));
                            stem.line_to(pt(0.50, 0.72));
                            if let Ok(path) = stem.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Play => {
                            let mut p = PathBuilder::fill();
                            p.move_to(pt(0.28, 0.20));
                            p.line_to(pt(0.80, 0.50));
                            p.line_to(pt(0.28, 0.80));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Stop => {
                            let quad = fill(
                                Bounds {
                                    origin: pt(0.24, 0.24),
                                    size: size(px(w * 0.52), px(h * 0.52)),
                                },
                                icon_color,
                            );
                            window.paint_quad(quad);
                        }

                        IconType::Refresh => {
                            // Open circular arrow
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.50, 0.18));
                            p.cubic_bezier_to(pt(0.82, 0.50), pt(0.70, 0.18), pt(0.82, 0.32));
                            p.cubic_bezier_to(pt(0.50, 0.82), pt(0.82, 0.68), pt(0.68, 0.82));
                            p.cubic_bezier_to(pt(0.18, 0.50), pt(0.32, 0.82), pt(0.18, 0.68));
                            p.line_to(pt(0.18, 0.38));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Arrow head
                            let mut arrow = PathBuilder::fill();
                            arrow.move_to(pt(0.08, 0.42));
                            arrow.line_to(pt(0.18, 0.24));
                            arrow.line_to(pt(0.28, 0.42));
                            arrow.close();
                            if let Ok(path) = arrow.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Trash => {
                            // Can lid and bin
                            let mut p = PathBuilder::stroke(stroke_w);
                            // Lid
                            p.move_to(pt(0.20, 0.26));
                            p.line_to(pt(0.80, 0.26));
                            p.move_to(pt(0.38, 0.18));
                            p.line_to(pt(0.62, 0.18));
                            // Body
                            p.move_to(pt(0.28, 0.26));
                            p.line_to(pt(0.32, 0.82));
                            p.line_to(pt(0.68, 0.82));
                            p.line_to(pt(0.72, 0.26));
                            // Vertical rib
                            p.move_to(pt(0.50, 0.36));
                            p.line_to(pt(0.50, 0.72));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Edit => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.20, 0.80));
                            p.line_to(pt(0.36, 0.80));
                            p.line_to(pt(0.80, 0.36));
                            p.line_to(pt(0.64, 0.20));
                            p.line_to(pt(0.20, 0.64));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Copy => {
                            // Back sheet
                            let mut p1 = PathBuilder::stroke(stroke_w);
                            p1.move_to(pt(0.32, 0.20));
                            p1.line_to(pt(0.80, 0.20));
                            p1.line_to(pt(0.80, 0.68));
                            if let Ok(path) = p1.build() {
                                window.paint_path(path, icon_color);
                            }

                            // Front sheet
                            let mut p2 = PathBuilder::stroke(stroke_w);
                            p2.move_to(pt(0.20, 0.32));
                            p2.line_to(pt(0.68, 0.32));
                            p2.line_to(pt(0.68, 0.80));
                            p2.line_to(pt(0.20, 0.80));
                            p2.close();
                            if let Ok(path) = p2.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Save => {
                            // Floppy / Memory cartridge
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.18));
                            p.line_to(pt(0.68, 0.18));
                            p.line_to(pt(0.82, 0.32));
                            p.line_to(pt(0.82, 0.82));
                            p.line_to(pt(0.18, 0.82));
                            p.close();
                            // Top shutter
                            p.move_to(pt(0.32, 0.18));
                            p.line_to(pt(0.32, 0.42));
                            p.line_to(pt(0.60, 0.42));
                            p.line_to(pt(0.60, 0.18));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Undo => {
                            // Curved revert arrow
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.30, 0.42));
                            p.cubic_bezier_to(pt(0.78, 0.72), pt(0.50, 0.30), pt(0.78, 0.46));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }

                            let mut head = PathBuilder::fill();
                            head.move_to(pt(0.18, 0.42));
                            head.line_to(pt(0.36, 0.26));
                            head.line_to(pt(0.36, 0.58));
                            head.close();
                            if let Ok(path) = head.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Clear => {
                            // Diagonal broom sweep
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.75, 0.15));
                            p.line_to(pt(0.45, 0.52));
                            p.move_to(pt(0.32, 0.42));
                            p.line_to(pt(0.58, 0.62));
                            p.line_to(pt(0.42, 0.85));
                            p.line_to(pt(0.20, 0.75));
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::ArrowUp => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.50, 0.78));
                            p.line_to(pt(0.50, 0.22));
                            p.move_to(pt(0.26, 0.46));
                            p.line_to(pt(0.50, 0.22));
                            p.line_to(pt(0.74, 0.46));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::ArrowDown => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.50, 0.22));
                            p.line_to(pt(0.50, 0.78));
                            p.move_to(pt(0.26, 0.54));
                            p.line_to(pt(0.50, 0.78));
                            p.line_to(pt(0.74, 0.54));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Sort => {
                            let mut p = PathBuilder::stroke(stroke_w);
                            // Up arrow
                            p.move_to(pt(0.36, 0.75));
                            p.line_to(pt(0.36, 0.25));
                            p.move_to(pt(0.20, 0.42));
                            p.line_to(pt(0.36, 0.25));
                            p.line_to(pt(0.52, 0.42));
                            // Down arrow
                            p.move_to(pt(0.68, 0.25));
                            p.line_to(pt(0.68, 0.75));
                            p.move_to(pt(0.52, 0.58));
                            p.line_to(pt(0.68, 0.75));
                            p.line_to(pt(0.84, 0.58));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Grip => {
                            // 6-dot matrix drag handle
                            let r = px((w * 0.08).max(1.0));
                            for row in 0..3 {
                                for col in 0..2 {
                                    let x = 0.35 + (col as f32) * 0.30;
                                    let y = 0.25 + (row as f32) * 0.25;
                                    let dot = fill(
                                        Bounds {
                                            origin: pt(x, y),
                                            size: size(r * 2.0, r * 2.0),
                                        },
                                        icon_color,
                                    );
                                    window.paint_quad(dot);
                                }
                            }
                        }

                        IconType::Docker => {
                            // 3x2 container grid stack
                            let c_w = w * 0.20;
                            let c_h = h * 0.16;
                            // Bottom base boat
                            let mut hull = PathBuilder::stroke(stroke_w);
                            hull.move_to(pt(0.12, 0.72));
                            hull.line_to(pt(0.88, 0.72));
                            hull.cubic_bezier_to(pt(0.50, 0.88), pt(0.84, 0.85), pt(0.68, 0.88));
                            hull.cubic_bezier_to(pt(0.12, 0.72), pt(0.32, 0.88), pt(0.16, 0.85));
                            if let Ok(path) = hull.build() {
                                window.paint_path(path, icon_color);
                            }

                            // 3 cargo boxes
                            for i in 0..3 {
                                let origin = pt(0.24 + (i as f32) * 0.20, 0.50);
                                let box_q = fill(
                                    Bounds {
                                        origin,
                                        size: size(px(c_w), px(c_h)),
                                    },
                                    icon_color,
                                );
                                window.paint_quad(box_q);
                            }
                            // Top cargo box
                            let top_box = fill(
                                Bounds {
                                    origin: pt(0.44, 0.30),
                                    size: size(px(c_w), px(c_h)),
                                },
                                icon_color,
                            );
                            window.paint_quad(top_box);
                        }

                        IconType::Cpu => {
                            // Chip body
                            let mut chip = PathBuilder::stroke(stroke_w);
                            chip.move_to(pt(0.25, 0.25));
                            chip.line_to(pt(0.75, 0.25));
                            chip.line_to(pt(0.75, 0.75));
                            chip.line_to(pt(0.25, 0.75));
                            chip.close();
                            // Inner core
                            chip.move_to(pt(0.40, 0.40));
                            chip.line_to(pt(0.60, 0.40));
                            chip.line_to(pt(0.60, 0.60));
                            chip.line_to(pt(0.40, 0.60));
                            chip.close();

                            // Pin connectors (top, bottom, left, right)
                            chip.move_to(pt(0.35, 0.12));
                            chip.line_to(pt(0.35, 0.25));
                            chip.move_to(pt(0.65, 0.12));
                            chip.line_to(pt(0.65, 0.25));
                            chip.move_to(pt(0.35, 0.75));
                            chip.line_to(pt(0.35, 0.88));
                            chip.move_to(pt(0.65, 0.75));
                            chip.line_to(pt(0.65, 0.88));

                            chip.move_to(pt(0.12, 0.35));
                            chip.line_to(pt(0.25, 0.35));
                            chip.move_to(pt(0.12, 0.65));
                            chip.line_to(pt(0.25, 0.65));
                            chip.move_to(pt(0.75, 0.35));
                            chip.line_to(pt(0.88, 0.35));
                            chip.move_to(pt(0.75, 0.65));
                            chip.line_to(pt(0.88, 0.65));

                            if let Ok(path) = chip.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Memory => {
                            // Dual RAM sticks
                            let mut ram = PathBuilder::stroke(stroke_w);
                            ram.move_to(pt(0.20, 0.20));
                            ram.line_to(pt(0.80, 0.20));
                            ram.line_to(pt(0.80, 0.48));
                            ram.line_to(pt(0.20, 0.48));
                            ram.close();
                            ram.move_to(pt(0.20, 0.56));
                            ram.line_to(pt(0.80, 0.56));
                            ram.line_to(pt(0.80, 0.84));
                            ram.line_to(pt(0.20, 0.84));
                            ram.close();
                            if let Ok(path) = ram.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Disk => {
                            // Hard drive enclosure
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.15, 0.20));
                            p.line_to(pt(0.85, 0.20));
                            p.line_to(pt(0.85, 0.80));
                            p.line_to(pt(0.15, 0.80));
                            p.close();

                            // Platter ring
                            let steps = 8;
                            for i in 0..steps {
                                let a = (i as f32) * std::f32::consts::PI * 2.0 / (steps as f32);
                                let pos = pt(0.50 + a.cos() * 0.18, 0.45 + a.sin() * 0.18);
                                if i == 0 {
                                    p.move_to(pos);
                                } else {
                                    p.line_to(pos);
                                }
                            }
                            p.close();
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Network => {
                            // 3 nodes in triangle + links
                            let mut links = PathBuilder::stroke(stroke_w);
                            links.move_to(pt(0.50, 0.22));
                            links.line_to(pt(0.22, 0.75));
                            links.move_to(pt(0.50, 0.22));
                            links.line_to(pt(0.78, 0.75));
                            links.move_to(pt(0.22, 0.75));
                            links.line_to(pt(0.78, 0.75));
                            if let Ok(path) = links.build() {
                                window.paint_path(path, icon_color);
                            }

                            let r = px(w * 0.10);
                            let n1 = fill(
                                Bounds {
                                    origin: pt(0.40, 0.12),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            let n2 = fill(
                                Bounds {
                                    origin: pt(0.12, 0.65),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            let n3 = fill(
                                Bounds {
                                    origin: pt(0.68, 0.65),
                                    size: size(r * 2.0, r * 2.0),
                                },
                                icon_color,
                            );
                            window.paint_quad(n1);
                            window.paint_quad(n2);
                            window.paint_quad(n3);
                        }

                        IconType::Tunnel => {
                            // Bidirectional arrows / routing pipeline
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.15, 0.38));
                            p.line_to(pt(0.85, 0.38));
                            p.line_to(pt(0.65, 0.22));
                            p.move_to(pt(0.85, 0.62));
                            p.line_to(pt(0.15, 0.62));
                            p.line_to(pt(0.35, 0.78));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Snippet => {
                            // Code angle brackets: < / >
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.32, 0.32));
                            p.line_to(pt(0.18, 0.50));
                            p.line_to(pt(0.32, 0.68));
                            p.move_to(pt(0.68, 0.32));
                            p.line_to(pt(0.82, 0.50));
                            p.line_to(pt(0.68, 0.68));
                            p.move_to(pt(0.58, 0.28));
                            p.line_to(pt(0.42, 0.72));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Linux => {
                            // Clean Linux Shell glyph `$_`
                            let mut p = PathBuilder::stroke(stroke_w);
                            // Dollar sign
                            p.move_to(pt(0.45, 0.26));
                            p.cubic_bezier_to(pt(0.25, 0.38), pt(0.25, 0.26), pt(0.25, 0.38));
                            p.cubic_bezier_to(pt(0.45, 0.62), pt(0.45, 0.48), pt(0.45, 0.62));
                            p.cubic_bezier_to(pt(0.25, 0.74), pt(0.25, 0.62), pt(0.25, 0.74));
                            p.move_to(pt(0.35, 0.20));
                            p.line_to(pt(0.35, 0.80));

                            // Underscore cursor
                            p.move_to(pt(0.55, 0.74));
                            p.line_to(pt(0.80, 0.74));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Apple => {
                            // Mac Command symbol `⌘`
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.32, 0.32));
                            p.line_to(pt(0.68, 0.32));
                            p.line_to(pt(0.68, 0.68));
                            p.line_to(pt(0.32, 0.68));
                            p.close();

                            // 4 Corner loops
                            p.move_to(pt(0.32, 0.32));
                            p.cubic_bezier_to(pt(0.32, 0.16), pt(0.16, 0.32), pt(0.16, 0.16));
                            p.cubic_bezier_to(pt(0.32, 0.32), pt(0.32, 0.16), pt(0.16, 0.32));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Windows => {
                            // 4-Quadrant tech window grid `⊞`
                            let quad_size = size(px(w * 0.34), px(h * 0.34));
                            let gap = 0.12;

                            let q1 = fill(
                                Bounds {
                                    origin: pt(gap, gap),
                                    size: quad_size,
                                },
                                icon_color,
                            );
                            let q2 = fill(
                                Bounds {
                                    origin: pt(0.54, gap),
                                    size: quad_size,
                                },
                                icon_color,
                            );
                            let q3 = fill(
                                Bounds {
                                    origin: pt(gap, 0.54),
                                    size: quad_size,
                                },
                                icon_color,
                            );
                            let q4 = fill(
                                Bounds {
                                    origin: pt(0.54, 0.54),
                                    size: quad_size,
                                },
                                icon_color,
                            );

                            window.paint_quad(q1);
                            window.paint_quad(q2);
                            window.paint_quad(q3);
                            window.paint_quad(q4);
                        }

                        IconType::Link => {
                            // Interlinked chain glyph
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.24, 0.44));
                            p.line_to(pt(0.24, 0.26));
                            p.line_to(pt(0.48, 0.26));
                            p.line_to(pt(0.56, 0.34));

                            p.move_to(pt(0.36, 0.64));
                            p.line_to(pt(0.64, 0.36));

                            p.move_to(pt(0.44, 0.66));
                            p.line_to(pt(0.52, 0.74));
                            p.line_to(pt(0.76, 0.74));
                            p.line_to(pt(0.76, 0.56));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Archive => {
                            // Package / archive box with zipper
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.24));
                            p.line_to(pt(0.82, 0.24));
                            p.line_to(pt(0.82, 0.82));
                            p.line_to(pt(0.18, 0.82));
                            p.close();

                            // Top seam
                            p.move_to(pt(0.18, 0.42));
                            p.line_to(pt(0.82, 0.42));
                            // Center zipper
                            p.move_to(pt(0.50, 0.42));
                            p.line_to(pt(0.50, 0.82));
                            p.move_to(pt(0.42, 0.54));
                            p.line_to(pt(0.58, 0.54));
                            p.move_to(pt(0.42, 0.68));
                            p.line_to(pt(0.58, 0.68));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Download => {
                            // Download tray + arrow down
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.66));
                            p.line_to(pt(0.18, 0.84));
                            p.line_to(pt(0.82, 0.84));
                            p.line_to(pt(0.82, 0.66));

                            p.move_to(pt(0.50, 0.16));
                            p.line_to(pt(0.50, 0.64));
                            p.move_to(pt(0.32, 0.46));
                            p.line_to(pt(0.50, 0.64));
                            p.line_to(pt(0.68, 0.46));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Upload => {
                            // Upload tray + arrow up
                            let mut p = PathBuilder::stroke(stroke_w);
                            p.move_to(pt(0.18, 0.66));
                            p.line_to(pt(0.18, 0.84));
                            p.line_to(pt(0.82, 0.84));
                            p.line_to(pt(0.82, 0.66));

                            p.move_to(pt(0.50, 0.64));
                            p.line_to(pt(0.50, 0.16));
                            p.move_to(pt(0.32, 0.34));
                            p.line_to(pt(0.50, 0.16));
                            p.line_to(pt(0.68, 0.34));
                            if let Ok(path) = p.build() {
                                window.paint_path(path, icon_color);
                            }
                        }

                        IconType::Clock => {
                            // Circular clock face
                            let mut ring = PathBuilder::stroke(stroke_w);
                            let steps = 12;
                            for i in 0..steps {
                                let angle =
                                    (i as f32) * std::f32::consts::PI * 2.0 / (steps as f32);
                                let pt_pos = pt(0.5 + angle.cos() * 0.36, 0.5 + angle.sin() * 0.36);
                                if i == 0 {
                                    ring.move_to(pt_pos);
                                } else {
                                    ring.line_to(pt_pos);
                                }
                            }
                            ring.close();

                            // Hour & minute hands
                            ring.move_to(pt(0.50, 0.50));
                            ring.line_to(pt(0.50, 0.26));
                            ring.move_to(pt(0.50, 0.50));
                            ring.line_to(pt(0.70, 0.50));
                            if let Ok(path) = ring.build() {
                                window.paint_path(path, icon_color);
                            }
                        }
                    }
                },
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_icon_creation() {
        let icon = Icon::terminal().with_size(px(16.0));
        assert_eq!(icon.icon_type, IconType::Terminal);
        assert_eq!(icon.size, px(16.0));
    }

    #[core::prelude::v1::test]
    fn test_all_icon_constructors() {
        assert_eq!(Icon::server().icon_type, IconType::Server);
        assert_eq!(Icon::settings().icon_type, IconType::Settings);
        assert_eq!(Icon::zap().icon_type, IconType::Zap);
        assert_eq!(Icon::folder().icon_type, IconType::Folder);
        assert_eq!(Icon::file().icon_type, IconType::File);
        assert_eq!(Icon::search().icon_type, IconType::Search);
        assert_eq!(Icon::plus().icon_type, IconType::Plus);
        assert_eq!(Icon::close().icon_type, IconType::Close);
        assert_eq!(Icon::check().icon_type, IconType::Check);
        assert_eq!(Icon::docker().icon_type, IconType::Docker);
        assert_eq!(Icon::cpu().icon_type, IconType::Cpu);
        assert_eq!(Icon::memory().icon_type, IconType::Memory);
        assert_eq!(Icon::disk().icon_type, IconType::Disk);
        assert_eq!(Icon::network().icon_type, IconType::Network);
        assert_eq!(Icon::tunnel().icon_type, IconType::Tunnel);
        assert_eq!(Icon::snippet().icon_type, IconType::Snippet);
        assert_eq!(Icon::linux().icon_type, IconType::Linux);
        assert_eq!(Icon::apple().icon_type, IconType::Apple);
        assert_eq!(Icon::windows().icon_type, IconType::Windows);
        assert_eq!(Icon::link().icon_type, IconType::Link);
        assert_eq!(Icon::archive().icon_type, IconType::Archive);
        assert_eq!(Icon::download().icon_type, IconType::Download);
        assert_eq!(Icon::upload().icon_type, IconType::Upload);
        assert_eq!(Icon::clock().icon_type, IconType::Clock);
    }
}
