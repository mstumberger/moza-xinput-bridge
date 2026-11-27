//! Configuration: persistence, default R3 mapping, calibration.
//!
//! Config lives in `app.json` next to the exe when that directory is
//! writable, otherwise in `%APPDATA%\moza_mapper\`. Load errors are
//! surfaced to the caller (GUI) instead of being swallowed.

use std::path::{Path, PathBuf};

/// Pedal direction: which way the raw value moves away from the 0x8000
/// idle when pressed. Verified live (2026-08-26): clutch on bytes 15-16
/// with Up decodes correctly, and gas/brake show the classic
/// downward-transform inversion — ALL R3 pedals travel UP from center.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PedalDirection {
    #[default]
    Up,
    Down,
}

fn default_up() -> PedalDirection {
    PedalDirection::Up
}

/// Canonical per-axis calibration (min, max) raw pedal values.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Calibration {
    pub throttle: (u16, u16),
    pub brake: (u16, u16),
    pub clutch: (u16, u16),
    #[serde(default)]
    pub throttle_dir: PedalDirection,
    #[serde(default)]
    pub brake_dir: PedalDirection,
    #[serde(default = "default_up")]
    pub clutch_dir: PedalDirection,
}

/// Default calibration: pedals idle at raw 0x8000; the value fed to
/// calibration is the wraparound-continuous travel distance in the pedal's
/// direction, so full travel is 0..65535.
impl Default for Calibration {
    fn default() -> Self {
        Self {
            throttle: (0, 65535),
            brake: (0, 65535),
            clutch: (0, 65535),
            throttle_dir: PedalDirection::Up,
            brake_dir: PedalDirection::Up,
            clutch_dir: PedalDirection::Up,
        }
    }
}

/// Xbox button names in canonical bit order (ViGEm X360 report order).
pub const XBOX_ORDER: [&str; 15] = [
    "A", "B", "X", "Y", "DPAD_UP", "DPAD_RIGHT", "DPAD_DOWN", "DPAD_LEFT", "LB", "RB", "Guide",
    "Back", "Start", "LS", "RS",
];

/// One mapped button. `moza_button` is 1-based (Joystick Gremlin numbering
/// on the MOZA R3); 0 means unmapped.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MappedButton {
    pub xbox: String,
    pub moza_button: u8,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct MappingConfig {
    pub buttons: Vec<MappedButton>,
}

impl MappingConfig {
    /// Default MOZA R3 mapping (Joystick Gremlin 1-based button numbers).
    pub fn default_r3() -> Self {
        // (xbox name, 1-based MOZA button) in XBOX_ORDER order.
        const TABLE: [(&str, u8); 15] = [
            ("A", 1),
            ("B", 2),
            ("X", 3),
            ("Y", 4),
            ("DPAD_UP", 5),
            ("DPAD_RIGHT", 6),
            ("DPAD_DOWN", 7),
            ("DPAD_LEFT", 8),
            ("LB", 17),
            ("RB", 18),
            ("Guide", 38),
            ("Back", 35),
            ("Start", 36),
            ("LS", 33),
            ("RS", 34),
        ];
        debug_assert_eq!(TABLE.len(), XBOX_ORDER.len());
        Self {
            buttons: TABLE
                .iter()
                .map(|&(xbox, moza)| MappedButton {
                    xbox: xbox.to_string(),
                    moza_button: moza,
                })
                .collect(),
        }
    }

    /// 1-based MOZA button index for an xbox button name, 0 if unmapped.
    pub fn lookup(&self, xbox: &str) -> u8 {
        self.buttons
            .iter()
            .find(|b| b.xbox == xbox)
            .map(|b| b.moza_button)
            .unwrap_or(0)
    }
}

/// RPM shift-light settings. Thresholds are evenly spaced from `start_pct`
/// to `end_pct` of `max_rpm` across `num_leds` lights.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RpmLightsConfig {
    pub enabled: bool,
    pub max_rpm: u16,
    pub num_leds: u8,
    pub start_pct: u8,
    pub end_pct: u8,
    pub udp_enabled: bool,
    pub udp_port: u16,
    /// Forward the virtual controller's rumble (large motor) onto the light
    /// bar. XInput has no RPM field; this is an approximation for games
    /// that rumble near redline.
    pub rumble_enabled: bool,
}

impl Default for RpmLightsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_rpm: 9_000,
            num_leds: 10,
            start_pct: 55,
            end_pct: 100,
            udp_enabled: true,
            udp_port: 26999,
            rumble_enabled: false,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub mapping: MappingConfig,
    pub calibration: Calibration,
    pub ffb_enabled: bool,
    pub ffb_gain: f32,
    /// Absent in older configs -> defaults (lights off).
    #[serde(default)]
    pub rpm_lights: RpmLightsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mapping: MappingConfig::default_r3(),
            calibration: Calibration::default(),
            ffb_enabled: true,
            ffb_gain: 1.0,
            rpm_lights: RpmLightsConfig::default(),
        }
    }
}

/// Directory holding `app.json`: exe directory if writable, else
/// `%APPDATA%\moza_mapper` (created).
pub fn config_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir_is_writable(dir) {
                return dir.to_path_buf();
            }
        }
    }
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir())
        .join("moza_mapper");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("failed to create config dir {:?}: {}", dir, e);
    }
    dir
}

fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".moza_mapper_write_probe_{}", std::process::id()));
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(e) => {
            log::info!("config dir {:?} not writable ({}), falling back to APPDATA", dir, e);
            false
        }
    }
}

impl AppConfig {
    /// Load `app.json` from [`config_dir`]. Missing file -> default.
    /// Parse/IO errors -> `Err(msg)` for the GUI to show.
    pub fn load() -> Result<AppConfig, String> {
        Self::load_from(&config_dir())
    }

    /// Load, falling back to the default config; returns the error (if any)
    /// so the GUI can display it without losing the default config.
    pub fn load_or_default() -> (AppConfig, Option<String>) {
        Self::load_or_default_from(&config_dir())
    }

    fn load_or_default_from(dir: &Path) -> (AppConfig, Option<String>) {
        match Self::load_from(dir) {
            Ok(cfg) => (cfg, None),
            Err(e) => (AppConfig::default(), Some(e)),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&config_dir())
    }

    fn load_from(dir: &Path) -> Result<AppConfig, String> {
        let path = dir.join("app.json");
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("no config file at {:?}; using defaults", path);
                Ok(AppConfig::default())
            }
            Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("failed to parse {}: {}", path.display(), e)),
        }
    }

    fn save_to(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join("app.json");
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize config: {}", e))?;
        // Atomic write: temp file + rename, so a crash mid-write can never
        // leave a truncated app.json behind.
        let tmp = dir.join("app.json.tmp");
        std::fs::write(&tmp, &text)
            .map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("failed to rename into {}: {}", path.display(), e)
        })
    }
}

/// Import the legacy `button_mapping.json` format. Errors are surfaced, not
/// swallowed.
pub fn import_legacy_button_mapping(path: &Path) -> Result<MappingConfig, String> {
    #[derive(serde::Deserialize)]
    struct LegacyEntry {
        xbox_button: String,
        moza_button_index: usize,
    }
    #[derive(serde::Deserialize)]
    struct LegacyFile {
        button_mappings: Vec<LegacyEntry>,
    }

    // Legacy xbox names -> canonical XBOX_ORDER names.
    fn canon(name: &str) -> Option<&'static str> {
        match name {
            "DUp" => Some("DPAD_UP"),
            "DDown" => Some("DPAD_DOWN"),
            "DLeft" => Some("DPAD_LEFT"),
            "DRight" => Some("DPAD_RIGHT"),
            n => XBOX_ORDER.iter().find(|x| **x == n).copied(),
        }
    }

    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read legacy {}: {}", path.display(), e))?;
    let legacy: LegacyFile = serde_json::from_str(&text)
        .map_err(|e| format!("failed to parse legacy {}: {}", path.display(), e))?;
    let buttons = legacy
        .button_mappings
        .into_iter()
        .filter_map(|e| {
            canon(&e.xbox_button).map(|xbox| MappedButton {
                xbox: xbox.to_string(),
                // Legacy indices were 0-based array indices; 0 = unmapped.
                moza_button: if e.moza_button_index == 0 { 0 } else { (e.moza_button_index + 1) as u8 },
            })
        })
        .collect();
    Ok(MappingConfig { buttons })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("moza_mapper_config_tests")
            .join(format!("{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_r3_lookups() {
        let m = MappingConfig::default_r3();
        assert_eq!(m.lookup("A"), 1);
        assert_eq!(m.lookup("B"), 2);
        assert_eq!(m.lookup("X"), 3);
        assert_eq!(m.lookup("Y"), 4);
        assert_eq!(m.lookup("DPAD_UP"), 5);
        assert_eq!(m.lookup("DPAD_RIGHT"), 6);
        assert_eq!(m.lookup("DPAD_DOWN"), 7);
        assert_eq!(m.lookup("DPAD_LEFT"), 8);
        assert_eq!(m.lookup("LB"), 17);
        assert_eq!(m.lookup("RB"), 18);
        assert_eq!(m.lookup("Guide"), 38);
        assert_eq!(m.lookup("Back"), 35);
        assert_eq!(m.lookup("Start"), 36);
        assert_eq!(m.lookup("LS"), 33);
        assert_eq!(m.lookup("RS"), 34);
        assert_eq!(m.lookup("NotAButton"), 0);
    }

    #[test]
    fn config_dir_exists() {
        let dir = config_dir();
        assert!(dir.is_dir(), "config_dir {:?} should exist", dir);
    }

    #[test]
    fn round_trip_save_load() {
        let dir = temp_subdir("roundtrip");
        let mut cfg = AppConfig::default();
        cfg.ffb_gain = 0.75;
        cfg.ffb_enabled = false;
        cfg.calibration.brake = (100, 60000);
        cfg.mapping
            .buttons
            .iter_mut()
            .find(|b| b.xbox == "B")
            .unwrap()
            .moza_button = 9;
        cfg.save_to(&dir).unwrap();
        let loaded = AppConfig::load_from(&dir).unwrap();
        assert_eq!(loaded.ffb_gain, 0.75);
        assert!(!loaded.ffb_enabled);
        assert_eq!(loaded.calibration.brake, (100, 60000));
        assert_eq!(loaded.mapping.lookup("B"), 9);
        assert_eq!(loaded.mapping.lookup("A"), 1, "default entry preserved");
        assert_eq!(loaded.mapping.buttons.len(), cfg.mapping.buttons.len());
    }

    #[test]
    fn missing_file_yields_default() {
        let dir = temp_subdir("missing");
        let (cfg, err) = {
            match AppConfig::load_from(&dir) {
                Ok(c) => (c, None),
                Err(e) => (AppConfig::default(), Some(e)),
            }
        };
        assert!(err.is_none());
        assert_eq!(cfg.mapping.lookup("A"), 1);
        assert!(cfg.ffb_enabled);
        assert_eq!(cfg.ffb_gain, 1.0);
    }

    #[test]
    fn corrupt_json_is_err_and_default_recovery() {
        let dir = temp_subdir("corrupt");
        std::fs::write(dir.join("app.json"), "{ not json !!!").unwrap();
        assert!(AppConfig::load_from(&dir).is_err());
        // load_or_default must still yield a usable default with the error.
        let (cfg, err) = AppConfig::load_or_default_from(&dir);
        assert!(err.is_some(), "corrupt config should surface an error");
        assert_eq!(cfg.mapping.lookup("A"), 1);
    }

    #[test]
    fn legacy_import_maps_old_names() {
        let dir = temp_subdir("legacy");
        let legacy = r#"{
            "button_mappings": [
                {"xbox_button": "A", "moza_button_index": 3, "description": "x"},
                {"xbox_button": "DUp", "moza_button_index": 6, "description": "x"},
                {"xbox_button": "LT", "moza_button_index": 2, "description": "analog, ignored"}
            ]
        }"#;
        let path = dir.join("button_mapping.json");
        std::fs::write(&path, legacy).unwrap();
        let m = import_legacy_button_mapping(&path).expect("legacy import should succeed");
        // 0-based legacy index -> 1-based moza button.
        assert_eq!(m.lookup("A"), 4);
        assert_eq!(m.lookup("DPAD_UP"), 7);
        assert_eq!(m.lookup("LT"), 0, "analog triggers are not mapped");
    }

    #[test]
    fn legacy_import_bad_path_is_none() {
        assert!(import_legacy_button_mapping(Path::new(
            "Z:/definitely/not/here.json"
        ))
        .is_err());
    }
}
