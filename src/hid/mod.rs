//! HID layer: MOZA device discovery, report-descriptor parsing, and
//! descriptor-driven input report decoding.

pub mod descriptor;
pub mod device;
pub mod state;

#[allow(unused_imports)]
pub use descriptor::{AxisLayout, ButtonRange, ReportLayout};
#[allow(unused_imports)]
pub use device::{fallback_layout, open_moza, open_moza_with_layout};
#[allow(unused_imports)]
pub use state::{decode, Calibration, DeviceState};

/// MOZA vendor id.
pub const MOZA_VID: u16 = 0x346E;
/// Known MOZA wheel-base product ids (R3 and variants).
pub const MOZA_PIDS: [u16; 2] = [0x0005, 0x0015];
