pub mod chart;
pub mod host_modal;
pub mod icon;
pub mod micro_meter;
pub mod status_led;
pub mod tooltip;

pub use crate::theme;

impl gpui::IntoElement for status_led::StatusLed {
    type Element = gpui::Component<Self>;
    fn into_element(self) -> Self::Element {
        gpui::Component::new(self)
    }
}

impl gpui::IntoElement for micro_meter::MicroMeter {
    type Element = gpui::Component<Self>;
    fn into_element(self) -> Self::Element {
        gpui::Component::new(self)
    }
}
