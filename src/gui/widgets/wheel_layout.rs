//! Button positions of the drawn ES wheel (spec 7.3) and the short Xbox
//! names shown on the buttons (spec 7.4).

use crate::config::MappingConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotKind {
    Round,
    Small,
    Large,
    /// Lights the whole paddle shape instead of drawing a circle.
    Paddle,
}

impl SpotKind {
    /// Circle radius in normalized wheel units (0 for paddles).
    pub fn radius(self) -> f32 {
        match self {
            SpotKind::Round => 0.040,
            SpotKind::Small => 0.030,
            SpotKind::Large => 0.075,
            SpotKind::Paddle => 0.0,
        }
    }
}

pub struct ButtonSpot {
    pub button: u8,
    pub x: f32,
    pub y: f32,
    pub kind: SpotKind,
}

const fn spot(button: u8, x: f32, y: f32, kind: SpotKind) -> ButtonSpot {
    ButtonSpot { button, x, y, kind }
}

use SpotKind::{Large, Paddle, Round, Small};

/// Positions measured from Pit House's ES wheel page; numbers are the 1-based
/// wheel button numbers used everywhere in this app.
pub const ES_WHEEL_LAYOUT: &[ButtonSpot] = &[
    spot(1, 0.42, 0.12, Round),
    spot(2, 0.50, 0.04, Round),
    spot(3, 0.34, 0.04, Round),
    spot(4, 0.42, -0.04, Round),
    spot(5, -0.43, -0.04, Round),
    spot(6, -0.35, 0.04, Round),
    spot(7, -0.43, 0.12, Round),
    spot(8, -0.51, 0.04, Round),
    spot(13, -0.66, -0.22, Paddle),
    spot(14, 0.66, -0.22, Paddle),
    spot(19, -0.49, -0.14, Round),
    spot(20, -0.39, -0.19, Round),
    spot(21, -0.23, -0.11, Round),
    spot(22, -0.49, 0.26, Round),
    spot(23, -0.29, 0.39, Large),
    spot(24, -0.07, 0.34, Small),
    spot(25, 0.00, 0.43, Small),
    spot(32, 0.48, -0.14, Round),
    spot(33, 0.37, -0.19, Round),
    spot(34, 0.22, -0.11, Round),
    spot(35, 0.48, 0.26, Round),
    spot(36, 0.28, 0.39, Large),
    spot(37, 0.06, 0.34, Small),
    spot(38, 0.00, 0.52, Small),
];

/// Whether wheel button `n` has a drawn position.
pub fn has_spot(n: u8) -> bool {
    ES_WHEEL_LAYOUT.iter().any(|s| s.button == n)
}

/// Short on-wheel name for an Xbox button name from `config::XBOX_ORDER`.
pub fn short_name(xbox: &str) -> Option<&'static str> {
    Some(match xbox {
        "A" => "A",
        "B" => "B",
        "X" => "X",
        "Y" => "Y",
        "DPAD_UP" => "\u{25B2}",
        "DPAD_RIGHT" => "\u{25B6}",
        "DPAD_DOWN" => "\u{25BC}",
        "DPAD_LEFT" => "\u{25C0}",
        "LB" => "LB",
        "RB" => "RB",
        "LS" => "LS",
        "RS" => "RS",
        "Back" => "Back",
        "Start" => "Start",
        "Guide" => "Guide",
        _ => return None,
    })
}

/// Label per wheel button (index N-1) from the mapping; unmapped (0) skipped.
pub fn build_labels(mapping: &MappingConfig) -> [Option<&'static str>; 64] {
    let mut labels = [None; 64];
    for b in &mapping.buttons {
        if b.moza_button == 0 || b.moza_button > 64 {
            continue;
        }
        if let Some(name) = short_name(&b.xbox) {
            labels[b.moza_button as usize - 1] = Some(name);
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_buttons_and_numbers_in_range() {
        let mut seen = std::collections::HashSet::new();
        for s in ES_WHEEL_LAYOUT {
            assert!((1..=64).contains(&s.button), "button {}", s.button);
            assert!(seen.insert(s.button), "duplicate button {}", s.button);
        }
        assert_eq!(ES_WHEEL_LAYOUT.len(), 24);
    }

    #[test]
    fn non_paddle_spots_are_inside_the_rim() {
        for s in ES_WHEEL_LAYOUT.iter().filter(|s| s.kind != SpotKind::Paddle) {
            assert!(s.x * s.x + s.y * s.y < 0.80 * 0.80, "button {}", s.button);
        }
    }

    #[test]
    fn default_mapping_labels() {
        let labels = build_labels(&MappingConfig::default_r3());
        assert_eq!(labels[0], Some("A"));
        assert_eq!(labels[12], Some("LB"));
        assert_eq!(labels[13], Some("RB"));
        assert_eq!(labels[35], Some("Start"));
        assert_eq!(labels[8], None);
    }

    #[test]
    fn unmapped_entries_are_skipped() {
        let mut m = MappingConfig::default_r3();
        m.buttons[0].moza_button = 0;
        let labels = build_labels(&m);
        assert!(labels.iter().all(|l| *l != Some("A")));
    }
}
