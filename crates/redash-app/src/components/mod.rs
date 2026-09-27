pub mod chart;
pub mod host_modal;
pub mod icon;
pub mod micro_meter;
pub mod status_led;

pub use crate::theme;

#[allow(unused_imports)]
pub use chart::*;
#[allow(unused_imports)]
pub use host_modal::*;
#[allow(unused_imports)]
pub use icon::*;
#[allow(unused_imports)]
pub use micro_meter::*;
#[allow(unused_imports)]
pub use status_led::*;
#[allow(unused_imports)]
pub use theme::*;

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
