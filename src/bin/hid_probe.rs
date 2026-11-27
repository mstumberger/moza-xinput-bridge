//! Diagnostic tool for the MOZA R3 wheel.
//!
//! Usage:
//!   hid-probe            enumerate devices, stream decoded input reports for 10 s
//!   hid-probe --watch    stream until Ctrl+C
//!   hid-probe --ffb-test send a 2 s constant-force ramp, then zero
//!
//! Console app on purpose (no `windows_subsystem` attribute).

use std::time::{Duration, Instant};

use moza_xinput_bridge::config::Calibration;
use moza_xinput_bridge::hid::{self, state::decode};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let ffb_test = args.iter().any(|a| a == "--ffb-test");
    let torque_only = args.iter().any(|a| a == "--torque");

    if torque_only {
        run_torque_only();
        return;
    }
    let lib_channel = args.iter().any(|a| a == "--torque-lib");
    if lib_channel {
        run_torque_lib();
        return;
    }
    if args.iter().any(|a| a == "--enum") {
        run_enum_all();
        return;
    }
    if args.iter().any(|a| a == "--axislog") {
        run_axislog();
        return;
    }
    if args.iter().any(|a| a == "--pedals") {
        run_pedals();
        return;
    }
    if args.iter().any(|a| a == "--release") {
        run_release();
        return;
    }
    if args.iter().any(|a| a == "--torque-out") {
        run_torque_out();
        return;
    }
    let watch = args.iter().any(|a| a == "--watch");
    let raw_dump = args.iter().any(|a| a == "--raw");

    let api = hidapi::HidApi::new().expect("failed to initialize hidapi");

    if raw_dump {
        run_raw_dump(&api);
        return;
    }

    println!("== MOZA device enumeration ==");
    let mut found = 0;
    for info in api.device_list() {
        if info.vendor_id() != hid::MOZA_VID || !hid::MOZA_PIDS.contains(&info.product_id()) {
            continue;
        }
        found += 1;
        println!(
            "vid=0x{:04X} pid=0x{:04X} usage_page=0x{:04X} usage=0x{:04X} interface={} path={:?}",
            info.vendor_id(),
            info.product_id(),
            info.usage_page(),
            info.usage(),
            info.interface_number(),
            info.path(),
        );
    }
    if found == 0 {
        println!("no MOZA devices found (is Pit House closed?)");
        std::process::exit(1);
    }

    let (device, layout) = match hid::device::open_moza_with_layout(&api) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to open MOZA device: {}", e);
            std::process::exit(1);
        }
    };

    println!("\n== report layout in use ==");
    println!("{:#?}", layout);

    if ffb_test {
        run_ffb_test();
    }

    println!(
        "\n== streaming decoded input reports ({} ==",
        if watch { "Ctrl+C to stop" } else { "10 s" }
    );
    println!("press buttons one by one and note the 1-based numbers shown ==");
    let cal = Calibration::default();
    let deadline = if watch {
        None
    } else {
        Some(Instant::now() + Duration::from_secs(10))
    };
    let mut last_buttons = [false; 64];
    let mut buffer = [0u8; 64];
    let mut reports = 0u64;
    let start = Instant::now();
    loop {
        if let Some(d) = deadline {
            if Instant::now() >= d {
                break;
            }
        }
        match device.read_timeout(&mut buffer, 100) {
            Ok(0) => continue,
            Err(e) => {
                println!("read error (device unplugged?): {}", e);
                break;
            }
            Ok(len) => {
                reports += 1;
                let state = decode(&buffer[..len], &layout, &cal);
                for (i, &pressed) in state.buttons.iter().enumerate() {
                    if pressed && !last_buttons[i] {
                        println!(
                            "button {} pressed (raw {:02X?})",
                            i + 1,
                            &buffer[..len.min(20)]
                        );
                    }
                    last_buttons[i] = pressed;
                }
                if reports % 500 == 0 {
                    let hz = reports as f64 / start.elapsed().as_secs_f64();
                    println!(
                        "steer={:+.3} throttle={:.3} brake={:.3} clutch={:.3}  ({:.0} Hz)",
                        state.steer, state.throttle, state.brake, state.clutch, hz
                    );
                }
            }
        }
    }
    println!("done ({} reports)", reports);
}

fn run_raw_dump(api: &hidapi::HidApi) {
    let (device, _layout) = match hid::device::open_moza_with_layout(api) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to open MOZA device: {}", e);
            std::process::exit(1);
        }
    };

    println!("== feature report scan ==");

    println!("== Win32 HID caps / descriptor ==");
    for info in api.device_list() {
        if info.vendor_id() != hid::MOZA_VID || !hid::MOZA_PIDS.contains(&info.product_id()) {
            continue;
        }
        match moza_xinput_bridge::ffb::transport::WinHid::open(info.path()) {
            Ok(win) => {
                println!(
                    "  output_report_len={} feature_report_len={}",
                    win.output_report_len, win.feature_report_len
                );
                match win.report_descriptor() {
                    Ok(desc) => {
                        print!("  descriptor ({} bytes): ", desc.len());
                        for b in &desc {
                            print!("{:02X} ", b);
                        }
                        println!();
                        if let Ok(layout) =
                            moza_xinput_bridge::hid::descriptor::parse(&desc)
                        {
                            println!("  parsed layout: {:?}", layout);
                        }
                    }
                    Err(e) => println!("  descriptor read failed: {}", e),
                }
                // Which output report IDs does the device accept?
                println!("  output report id scan (SetOutputReport, len {}):", win.output_report_len);
                for rid in 1u8..=0x40 {
                    let mut buf = vec![rid];
                    buf.resize(win.output_report_len, 0);
                    match win.set_output_report(&buf) {
                        Ok(()) => println!("    id 0x{:02X} accepted", rid),
                        Err(e) => {
                            let msg = e.to_string();
                            if !msg.contains("parameter is incorrect") {
                                println!("    id 0x{:02X} -> {}", rid, msg);
                            }
                        }
                    }
                }
            }
            Err(e) => println!("  WinHid open failed: {}", e),
        }
    }
    for rid in 0x00u8..=0x40 {
        let mut buf = vec![rid; 64];
        match device.get_feature_report(&mut buf) {
            Ok(n) => println!("  feature 0x{:02X} readable ({} bytes): {:02X?}", rid, n, &buf[..n.min(16)]),
            Err(_) => {}
        }
    }

    println!("== output write length probe ==");
    // PID Device Control (Reset=0x01) padded to various lengths: hidapi on
    // Windows fails with "parameter is incorrect" when the buffer does not
    // match the device's output report length.
    for len in [2usize, 3, 4, 5, 8, 9, 16, 17, 20, 21, 32, 33, 64, 65, 128] {
        let mut buf = vec![0u8; len];
        if len > 1 {
            buf[1] = 0x0D; // guessed PID device-control report id
            buf[2.min(len - 1)] = 0x01; // Reset
        }
        match device.write(&buf) {
            Ok(n) => println!("  len {} -> OK ({} bytes written)", len, n),
            Err(e) => println!("  len {} -> ERR {}", len, e),
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("\n== raw input report dump (5 s, on change) ==");
    let mut last: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 128];
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        match device.read_timeout(&mut buffer, 50) {
            Ok(0) | Err(_) => continue,
            Ok(len) => {
                let cur = buffer[..len].to_vec();
                if cur != last {
                    println!("{:02X?}", cur);
                    last = cur;
                }
            }
        }
    }
}

#[derive(Default)]
struct SteerTrack {
    cur: u16,
    reference: u16,
    max_dev: u16,
}

fn tracker_reset(t: &std::sync::Arc<std::sync::Mutex<SteerTrack>>) {
    let mut t = t.lock().unwrap();
    t.reference = t.cur;
    t.max_dev = 0;
}

/// Isolated vendor-register torque test (no PID sweep, no effect engine):
/// Emergency release: a fresh process gets one applied episode — spend it
/// Guided pedal axis mapping: streams the seven u16 axes (bytes 3..16) and
fn run_enum_all() {
    let api = hidapi::HidApi::new().expect("hidapi");
    println!("== ALL HID devices ==");
    for d in api.device_list() {
        println!(
            "vid=0x{:04X} pid=0x{:04X} usage_page=0x{:04X} usage=0x{:04X} iface={} {:?} {:?}",
            d.vendor_id(),
            d.product_id(),
            d.usage_page(),
            d.usage(),
            d.interface_number(),
            d.manufacturer_string(),
            d.product_string(),
        );
    }
}
fn run_axislog() {
    println!("== axis logger (30 s - press a pedal SLOWLY 0..100..0) ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let (dev, _) = match hid::device::open_moza_with_layout(&api) {
        Ok(v) => v,
        Err(e) => {
            println!("open failed: {e}");
            return;
        }
    };
    let mut samples: Vec<[u16; 7]> = Vec::new();
    let mut buf = [0u8; 64];
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f64() < 30.0 {
        if let Ok(len) = dev.read_timeout(&mut buf, 20) {
            if len >= 17 {
                let mut axes = [0u16; 7];
                for i in 0..7 {
                    axes[i] = u16::from_le_bytes([buf[3 + i * 2], buf[4 + i * 2]]);
                }
                samples.push(axes);
            }
        }
    }
    let mut max_dev = [0u16; 7];
    for s in &samples {
        for i in 0..7 {
            let d = s[i].abs_diff(0x8000);
            if d > max_dev[i] {
                max_dev[i] = d;
            }
        }
    }
    // Reveal sweeps vs teleports: print the value-transition sequence of
    // any axis that moved (the "jumping pedals" signature would be violent
    // teleporting between extremes instead of a smooth ramp).
    for i in 0..7 {
        if max_dev[i] > 2000 {
            let mut transitions: Vec<u16> = Vec::new();
            let mut lastv = samples[0][i];
            transitions.push(lastv);
            for s in &samples {
                if s[i].abs_diff(lastv) > 40 {
                    transitions.push(s[i]);
                    lastv = s[i];
                }
            }
            let n = transitions.len();
            let tail: Vec<String> = transitions
                [n.saturating_sub(30)..]
                .iter()
                .map(|v| format!("{:04X}", v))
                .collect();
            println!(
                "  axis{}: {} transitions, last 30 values: {}",
                i + 1,
                n,
                tail.join(" ")
            );
        }
    }
    for (i, d) in max_dev.iter().enumerate() {
        println!(
            "  axis{} (bytes {}-{}): max deviation from center {} counts {}",
            i + 1,
            3 + i * 2,
            4 + i * 2,
            d,
            if *d > 200 { "<== MOVED" } else { "" }
        );
    }
    println!("samples: {}", samples.len());
}
fn run_pedals() {
    println!("== pedal axis mapper ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let (dev, _) = match hid::device::open_moza_with_layout(&api) {
        Ok(v) => v,
        Err(e) => {
            println!("open failed: {e} (close the Moza Mapper app and retry)");
            return;
        }
    };
    let axes = ["axis1 (b3-4)", "axis2 (b5-6)", "axis3 (b7-8)", "axis4 (b9-10)", "axis5 (b11-12)", "axis6 (b13-14)", "axis7 (b15-16)"];

    for pedal in ["THROTTLE", "BRAKE", "CLUTCH"] {
        println!("
--- press {pedal} fully and hold... (5 s) ---");
        let mut baseline = [0u16; 7];
        let mut buf = [0u8; 64];
        // settle baseline
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut n = 0;
        while std::time::Instant::now() < deadline {
            if let Ok(len) = dev.read_timeout(&mut buf, 20) {
                if len >= 17 {
                    for i in 0..7 {
                        baseline[i] = u16::from_le_bytes([buf[3 + i * 2], buf[4 + i * 2]]);
                    }
                    n += 1;
                }
            }
        }
        let mut max_dev = [0u16; 7];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            if let Ok(len) = dev.read_timeout(&mut buf, 20) {
                if len >= 17 {
                    for i in 0..7 {
                        let v = u16::from_le_bytes([buf[3 + i * 2], buf[4 + i * 2]]);
                        let d = v.abs_diff(baseline[i]);
                        if d > max_dev[i] {
                            max_dev[i] = d;
                        }
                    }
                }
            }
        }
        let _ = n;
        println!("  {pedal} movement per axis:");
        for (i, name) in axes.iter().enumerate() {
            if max_dev[i] > 2000 {
                println!("    {name}: {} counts  <== MOVES", max_dev[i]);
            }
        }
    }
    println!("
pedal mapping done");
}
fn run_release() {
    println!("== torque release (fresh-process zero episode) ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let path = api
        .device_list()
        .find(|i| i.vendor_id() == hid::MOZA_VID && hid::MOZA_PIDS.contains(&i.product_id()))
        .map(|i| i.path().to_owned())
        .expect("no MOZA device");
    let dev = api.open_path(&path).expect("open");
    for rid in [0x14u8, 0x18] {
        let report: Vec<u8> = std::iter::once(rid).chain(std::iter::repeat(0).take(50)).collect();
        for _ in 0..20 {
            let _ = dev.write(&report);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    println!("release sent — the wheel should be free now");
}
fn run_torque_only() {
    println!("== direct register torque test (hands off the wheel) ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let path = api
        .device_list()
        .find(|i| i.vendor_id() == hid::MOZA_VID && hid::MOZA_PIDS.contains(&i.product_id()))
        .map(|i| i.path().to_owned())
        .expect("no MOZA device");
    let win = moza_xinput_bridge::ffb::transport::WinHid::open(&path).expect("WinHid open");
    let api2 = hidapi::HidApi::new().expect("hidapi reader");
    let (read_dev, _) = hid::device::open_moza_with_layout(&api2).expect("read handle");
    let tracker: std::sync::Arc<std::sync::Mutex<SteerTrack>> =
        std::sync::Arc::new(std::sync::Mutex::new(SteerTrack::default()));
    let t2 = tracker.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        loop {
            if let Ok(len) = read_dev.read_timeout(&mut buf, 10) {
                if len >= 3 {
                    let raw = u16::from_le_bytes([buf[1], buf[2]]);
                    let mut t = t2.lock().unwrap();
                    t.cur = raw;
                    let dev = raw.abs_diff(t.reference);
                    if dev > t.max_dev {
                        t.max_dev = dev;
                    }
                }
            }
        }
    });
    let send = |report: Vec<u8>| {
        let mut buf = report;
        buf.resize(win.output_report_len.max(buf.len()), 0);
        let _ = win.set_output_report(&buf);
    };

    // For each candidate register/offset: strong +, strong -, zero.
    for id in [0x14u8, 0x15, 0x16, 0x17, 0x18] {
        for off in [1usize, 2] {
            for (val, label) in [(0x6000i32, "plus"), (-0x6000i32, "minus")] {
                tracker_reset(&tracker);
                let mut r = vec![0u8; 6];
                r[0] = id;
                let v = (val as u16).to_le_bytes();
                r[off] = v[0];
                r[off + 1] = v[1];
                send(r);
                std::thread::sleep(Duration::from_millis(700));
                let dev = tracker.lock().unwrap().max_dev;
                if dev > 300 {
                    println!(
                        "  0x{:02X} off{} {}: movement {} counts <== HIT",
                        id, off, label, dev
                    );
                }
                let mut r = vec![0u8; 6];
                r[0] = id;
                send(r);
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
    println!("register torque test done (hits above only)");
}

/// Interrupt-OUT torque test: hidapi write() with the EXACT 51-byte output
/// report length (the length Windows requires; the MOZA driver streams its
/// torque reports down this pipe).
fn run_torque_out() {
    println!("== interrupt-OUT torque test via hidapi write(51) ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let path = api
        .device_list()
        .find(|i| i.vendor_id() == hid::MOZA_VID && hid::MOZA_PIDS.contains(&i.product_id()))
        .map(|i| i.path().to_owned())
        .expect("no MOZA device");
    let dev = api.open_path(&path).expect("hidapi open");

    let api2 = hidapi::HidApi::new().expect("hidapi reader");
    let (read_dev, _) = hid::device::open_moza_with_layout(&api2).expect("read handle");
    let tracker: std::sync::Arc<std::sync::Mutex<SteerTrack>> =
        std::sync::Arc::new(std::sync::Mutex::new(SteerTrack::default()));
    let t2 = tracker.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        loop {
            if let Ok(len) = read_dev.read_timeout(&mut buf, 10) {
                if len >= 3 {
                    let raw = u16::from_le_bytes([buf[1], buf[2]]);
                    let mut t = t2.lock().unwrap();
                    t.cur = raw;
                    let dev = raw.abs_diff(t.reference);
                    if dev > t.max_dev {
                        t.max_dev = dev;
                    }
                }
            }
        }
    });

    // Continuous-stream test: never write zeros between phases — change the
    // torque value directly. If the base keeps applying, the "one shot per
    // open" behavior is really "one shot per zero-write".
    let phases: [(u8, f32, f64); 6] = [
        (0x14, 20.0, 1.0),
        (0x18, 20.0, 1.0), // hard switch to the other register mid-stream
        (0x14, 35.0, 1.0),
        (0x18, 35.0, 1.0),
        (0x14, 0.0, 0.5),  // explicit zero ONCE
        (0x14, 20.0, 1.0), // does force return after the zero?
    ];
    for (i, (rid, pct, secs)) in phases.iter().enumerate() {
        tracker_reset(&tracker);
        let torque = (pct / 100.0 * 32767.0) as i16;
        let b = torque.to_le_bytes();
        let mut report = vec![*rid, b[0], b[1]];
        report.resize(51, 0);
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < *secs {
            if dev.write(&report).is_err() {
                println!("  write error in phase {i}");
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let dev_moved = tracker.lock().unwrap().max_dev;
        println!(
            "  phase {i}: reg 0x{:02X} {:.0}% for {:.1}s -> movement {} counts {}",
            rid, pct, secs, dev_moved,
            if dev_moved > 300 { "<== FELT" } else { "" }
        );
    }
    // Release at the end.
    let zero: Vec<u8> = std::iter::once(0x14).chain(std::iter::repeat(0).take(50)).collect();
    let _ = dev.write(&zero);
    // Re-arm candidates after the one-shot episode is spent:
    // (b) full hidapi re-enumeration, (c) 20 s cooldown.
    println!("== re-arm tests ==");
    {
        let api3 = hidapi::HidApi::new().expect("re-enumerate");
        let dev2 = api3.open_path(&path).expect("reopen after enumerate");
        tracker_reset(&tracker);
        let tb = 8191i16.to_le_bytes();
        let mut report = vec![0x14, tb[0], tb[1]];
        report.resize(51, 0);
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < 0.6 {
            let _ = dev2.write(&report);
            std::thread::sleep(Duration::from_millis(5));
        }
        let m = tracker.lock().unwrap().max_dev;
        println!(
            "  after re-enumerate (0x14): movement {} counts {}",
            m,
            if m > 300 { "<== REARMED" } else { "" }
        );
        let z: Vec<u8> = std::iter::once(0x14).chain(std::iter::repeat(0).take(50)).collect();
        let _ = dev2.write(&z);
    }
    println!("  waiting 20s cooldown...");
    std::thread::sleep(Duration::from_secs(20));
    {
        tracker_reset(&tracker);
        let tb = 8191i16.to_le_bytes();
        let mut report = vec![0x18, tb[0], tb[1]];
        report.resize(51, 0);
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < 0.6 {
            let _ = dev.write(&report);
            std::thread::sleep(Duration::from_millis(5));
        }
        let z: Vec<u8> = std::iter::once(0x18).chain(std::iter::repeat(0).take(50)).collect();
        let _ = dev.write(&z);
        let m = tracker.lock().unwrap().max_dev;
        println!(
            "  after 20s cooldown (0x18): movement {} counts {}",
            m,
            if m > 300 { "<== REARMED" } else { "" }
        );
    }
    println!("interrupt-OUT test done");
}
/// Test the library TorqueChannel (the exact code path the GUI uses).
fn run_torque_lib() {
    println!("== library TorqueChannel pulse test (hands off!) ==");
    let api = hidapi::HidApi::new().expect("hidapi");
    let path = api
        .device_list()
        .find(|i| i.vendor_id() == hid::MOZA_VID && hid::MOZA_PIDS.contains(&i.product_id()))
        .map(|i| i.path().to_owned())
        .expect("no MOZA device");
    let ch = match moza_xinput_bridge::ffb::vendor::TorqueChannel::open(&path) {
        Ok(c) => c,
        Err(e) => {
            println!("open failed: {}", e);
            return;
        }
    };
    // objective movement tracking
    let api2 = hidapi::HidApi::new().expect("hidapi reader");
    let (read_dev, _) = hid::device::open_moza_with_layout(&api2).expect("read handle");
    let tracker: std::sync::Arc<std::sync::Mutex<SteerTrack>> =
        std::sync::Arc::new(std::sync::Mutex::new(SteerTrack::default()));
    let t2 = tracker.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        loop {
            if let Ok(len) = read_dev.read_timeout(&mut buf, 10) {
                if len >= 3 {
                    let raw = u16::from_le_bytes([buf[1], buf[2]]);
                    let mut t = t2.lock().unwrap();
                    t.cur = raw;
                    let dev = raw.abs_diff(t.reference);
                    if dev > t.max_dev {
                        t.max_dev = dev;
                    }
                }
            }
        }
    });
    for pct in [25.0f32, -25.0, 25.0] {
        tracker_reset(&tracker);
        let t = (pct / 100.0 * 32767.0) as i16;
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < 0.7 {
            if let Err(e) = ch.set_torque(t) {
                println!("  write error: {}", e);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = ch.set_torque(0);
        let (dev, signed) = {
            let t = tracker.lock().unwrap();
            (t.max_dev, t.cur as i32 - t.reference as i32)
        };
        println!(
            "  pulse {:+.0}%: max movement {} counts, net {:+} {}",
            pct,
            dev,
            signed,
            if dev > 300 { "<== MOVED" } else { "" }
        );
        std::thread::sleep(Duration::from_millis(600));
    }
    ch.stop();
    println!("lib channel test done");
}

fn run_ffb_test() {
    println!("\n== FFB variant sweep (watch steer_raw: movement = force applied) ==");
    let api = hidapi::HidApi::new().expect("re-init hidapi");
    let path = api
        .device_list()
        .find(|i| i.vendor_id() == hid::MOZA_VID && hid::MOZA_PIDS.contains(&i.product_id()))
        .map(|i| i.path().to_owned())
        .expect("no MOZA device");
    let win = match moza_xinput_bridge::ffb::transport::WinHid::open(&path) {
        Ok(w) => w,
        Err(e) => {
            println!("open failed: {}", e);
            return;
        }
    };
    // Reader thread: track current raw, reference position, and max
    // deviation from the reference since the last reset.
    let api2 = hidapi::HidApi::new().expect("hidapi for reader");
    let (read_dev, _) = hid::device::open_moza_with_layout(&api2).expect("open read handle");
    let steer_tracker: std::sync::Arc<
        std::sync::Mutex<SteerTrack>,
    > = std::sync::Arc::new(std::sync::Mutex::new(SteerTrack::default()));
    let tracker = steer_tracker.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        loop {
            if let Ok(len) = read_dev.read_timeout(&mut buf, 10) {
                if len >= 3 {
                    let raw = u16::from_le_bytes([buf[1], buf[2]]);
                    let mut t = tracker.lock().unwrap();
                    t.cur = raw;
                    let dev = raw.abs_diff(t.reference);
                    if dev > t.max_dev {
                        t.max_dev = dev;
                    }
                }
            }
        }
    });

    let send = |report: Vec<u8>| -> Result<(), String> {
        let mut buf = report;
        buf.resize(win.output_report_len, 0);
        win.set_output_report(&buf)
    };

    const SET_EFFECT: u8 = 0x01;
    const ENVELOPE: u8 = 0x02;
    const SET_CONSTANT: u8 = 0x05;
    const EFFECT_OP: u8 = 0x0A;
    const BLOCK_FREE: u8 = 0x0B;
    const DEVICE_CONTROL: u8 = 0x0C;
    const GAIN: u8 = 0x0D;

    struct Variant {
        name: &'static str,
        block: u8,
        enable_axis: u8,
        dir_enable: u8,
        loop_count: u8,
        extra_dc: Option<u8>,
        envelope: bool,
    }
    let variants = [
        Variant { name: "A base", block: 1, enable_axis: 0, dir_enable: 0, loop_count: 0, extra_dc: None, envelope: false },
        Variant { name: "B axis+dir", block: 1, enable_axis: 0x01, dir_enable: 0x03, loop_count: 0, extra_dc: None, envelope: false },
        Variant { name: "C loop=FF", block: 1, enable_axis: 0x01, dir_enable: 0x03, loop_count: 0xFF, extra_dc: None, envelope: false },
        Variant { name: "D dc=02", block: 1, enable_axis: 0x01, dir_enable: 0x03, loop_count: 0, extra_dc: Some(0x02), envelope: false },
        Variant { name: "E block=2", block: 2, enable_axis: 0x01, dir_enable: 0x03, loop_count: 0, extra_dc: None, envelope: false },
        Variant { name: "F envelope", block: 1, enable_axis: 0x01, dir_enable: 0x03, loop_count: 0, extra_dc: None, envelope: true },
    ];

    // Null control: measure ambient movement with no effect running.
    tracker_reset(&steer_tracker);
    std::thread::sleep(Duration::from_millis(1500));
    let ambient = steer_tracker.lock().unwrap().max_dev;
    println!("  ambient (no effect): steer movement = {} counts", ambient);

    for v in &variants {
        tracker_reset(&steer_tracker);
        let _ = send(vec![DEVICE_CONTROL, 0x03]); // stop all
        let _ = send(vec![BLOCK_FREE, v.block]);
        if let Some(dc) = v.extra_dc {
            let _ = send(vec![DEVICE_CONTROL, dc]);
        }
        let _ = send(vec![DEVICE_CONTROL, 0x04]); // enable actuators
        let _ = send(vec![GAIN, 0xFF, 0xFF]);
        // Set Effect: block, type=Constant(0x26), duration=infinite,
        // trigger repeat, sample period, gain, trigger button, axes, dir en.
        let _ = send(vec![
            SET_EFFECT, v.block, 0x26, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
            0xFF, v.enable_axis, v.dir_enable,
        ]);
        if v.envelope {
            let _ = send(vec![ENVELOPE, v.block, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00]);
        }
        let _ = send(vec![SET_CONSTANT, v.block, 0x00, 0x40]); // half force
        let _ = send(vec![EFFECT_OP, v.block, 0x01, v.loop_count]);
        std::thread::sleep(Duration::from_millis(1500));
        let dev = steer_tracker.lock().unwrap().max_dev;
        println!(
            "  variant {}: steer deviation = {} counts {}",
            v.name,
            dev,
            if dev > 500 { "<== WHEEL MOVED" } else { "" }
        );
        let _ = send(vec![EFFECT_OP, v.block, 0x00, 0x00]);
        let _ = send(vec![DEVICE_CONTROL, 0x03]);
        std::thread::sleep(Duration::from_millis(300));
    }
    println!("sweep done");

    // Vendor direct-torque probe: for each accepted-but-unidentified output
    // report ID and each payload offset, send a signed half-force value and
    // watch for real wheel movement.
    println!("== vendor register probe (0x14..0x18) ==");
    let mut combos: Vec<(u8, usize)> = Vec::new();
    for id in 0x14u8..=0x18 {
        for off in 1..=3usize {
            combos.push((id, off));
        }
    }
    for (id, off) in combos {
        tracker_reset(&steer_tracker);
        let mut r = vec![0u8; 8];
        r[0] = id;
        r[off] = 0x00;
        r[off + 1] = 0x40; // +0x4000 signed LE = half force
        let _ = send(r);
        std::thread::sleep(Duration::from_millis(1500));
        let dev = steer_tracker.lock().unwrap().max_dev;
        if dev > 200 {
            println!(
                "  id 0x{:02X} offset {} value 0x4000: movement {} counts <== HIT",
                id, off, dev
            );
        }
        // clear
        let mut r = vec![0u8; 8];
        r[0] = id;
        let _ = send(r);
        std::thread::sleep(Duration::from_millis(120));
    }
    println!("vendor probe done (only hits shown; silence = nothing moved)");

    // Dose-response on the hit register: if 0x18 is a real torque command,
    // movement must scale with the value.
    println!("== dose-response on report 0x18 ==");
    let doses: [(i32, &str); 5] = [
        (0x1000, "weak +"),
        (0x4000, "half +"),
        (0x7000, "strong +"),
        (0x0000, "zero"),
        (-0x4000i32, "half -"),
    ];
    for (val, label) in doses {
        tracker_reset(&steer_tracker);
        let v = (val as u16).to_le_bytes();
        let _ = send(vec![0x18, v[0], v[1]]);
        std::thread::sleep(Duration::from_millis(1000));
        let dev = steer_tracker.lock().unwrap().max_dev;
        println!("  {} (0x{:04X}): movement {} counts", label, val as u16, dev);
        let _ = send(vec![0x18, 0x00, 0x00]);
        std::thread::sleep(Duration::from_millis(300));
    }
    println!("dose-response done");

    // MOZA vendor FFB protocol, decoded from USBPcap capture of the MOZA
    // Windows driver: effect create (01/type 09), start (0a), continuous
    // torque (05 02 <i16>), stop (0a ...0300). Init via 0c/0d.
    println!("== vendor torque test: 3 s ramp using decoded protocol ==");
    {
        let _ = send(vec![0x0C, 0x08]);
        let _ = send(vec![0x0D, 0xFF]);
        let _ = send(vec![0x0C, 0x04]);
        // set effect block 2 type 09 (verbatim from capture)
        let _ = send(vec![
            0x01, 0x02, 0x09, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x04,
            0x28, 0x23, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        // set constant (block 1 layout seen in capture; zeros)
        let _ = send(vec![0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let _ = send(vec![0x0A, 0x02, 0x01, 0x01]); // start
        tracker_reset(&steer_tracker);
        // Pattern: 2 pulses POSITIVE (0.25s on / 0.25s off), 0.75s pause,
        // 2 pulses NEGATIVE, repeat 3 times. If direction alternates between
        // pairs, the sign bit is honored; if all pulses push the same way,
        // the field is magnitude-only.
        for _rep in 0..3 {
            for (sign, label) in [(1.0f64, "POSITIVE"), (-1.0f64, "NEGATIVE")] {
                println!("  -- two {} pulses --", label);
                for _p in 0..2 {
                    let start = Instant::now();
                    while start.elapsed().as_secs_f64() < 0.25 {
                        let torque = (sign * 15000.0) as i16;
                        let b = torque.to_le_bytes();
                        let _ = send(vec![0x05, 0x02, b[0], b[1]]);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    let _ = send(vec![0x05, 0x02, 0x00, 0x00]);
                    std::thread::sleep(Duration::from_millis(250));
                }
                std::thread::sleep(Duration::from_millis(750));
            }
        }
        let dev = steer_tracker.lock().unwrap().max_dev;
        println!("  torque ramp movement: {} counts {}", dev, if dev > 500 { "<== FFB WORKS" } else { "" });
        let _ = send(vec![0x05, 0x02, 0x00, 0x00]);
        let _ = send(vec![0x0A, 0x02, 0x03, 0x00]); // stop
        let _ = send(vec![0x0C, 0x03]); // stop all
    }
    println!("vendor torque test done");
}
