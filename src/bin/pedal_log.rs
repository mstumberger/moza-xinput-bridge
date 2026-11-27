//! Passive pedal logger: records all seven u16 axes (bytes 3..16) to CSV
//! for 90 seconds. The user presses gas/brake/clutch a few times each, in
//! any order — the offline analysis correlates which axis belongs to which
//! pedal and its direction.

use std::io::Write;
use std::time::Instant;

fn main() {
    let api = hidapi::HidApi::new().expect("hidapi init");
    let (dev, _layout) = moza_xinput_bridge::hid::device::open_moza_with_layout(&api)
        .expect("open MOZA wheel");

    let path = std::env::temp_dir().join("pedallog.csv");
    let mut out = std::fs::File::create(&path).expect("create csv");
    writeln!(out, "t,a1,a2,a3,a4,a5,a6,a7").unwrap();

    let mut buf = [0u8; 64];
    let mut last = [0u16; 7];
    let start = Instant::now();
    println!("logging 90 s to {} - press gas, brake, clutch a few times each, any order", path.display());
    while start.elapsed().as_secs_f64() < 90.0 {
        if let Ok(len) = dev.read_timeout(&mut buf, 20) {
            if len >= 17 {
                let mut axes = [0u16; 7];
                for i in 0..7 {
                    axes[i] = u16::from_le_bytes([buf[3 + i * 2], buf[4 + i * 2]]);
                }
                if axes.iter().zip(last.iter()).any(|(a, b)| a.abs_diff(*b) > 40) {
                    writeln!(
                        out,
                        "{:.3},{},{},{},{},{},{},{}",
                        start.elapsed().as_secs_f64(),
                        axes[0], axes[1], axes[2], axes[3], axes[4], axes[5], axes[6]
                    )
                    .unwrap();
                    last = axes;
                }
            }
        }
    }
    println!("done: {}", path.display());
}
