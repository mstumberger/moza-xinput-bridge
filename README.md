# moza-xinput-bridge

Use a **MOZA R3 racing wheel** as a virtual **Xbox 360 controller** on Windows,
with working **force feedback** speaking the wheelbase's native HID PID protocol.

Some games and tools only understand XInput gamepads. This bridge reads the
wheel, pedals and buttons directly over HID, feeds them into a virtual X360 pad
through the [ViGEm Bus Driver](https://github.com/nefarius/ViGEmBus), and
drives the wheelbase motor itself.

> Not affiliated with or endorsed by MOZA Racing. "MOZA" is used only to
> describe the hardware this software works with.

## Features

- **Full input bridge**: steering, throttle, brake, clutch and all wheel
  buttons to an X360 controller, polled at ~860 Hz.
- **Force feedback**: the exact PID report sequence a real game sends through
  the MOZA driver, decoded from USB captures and verified on hardware
  (see [docs/FFB.md](docs/FFB.md)).
- **FFB test panel**: push left/right at a chosen strength, live steering
  readout, and an automatic pulse test that *measures* wheel movement instead
  of relying on feel.
- **Button mapping wizard**: press the wheel button for each Xbox button;
  skipped entries keep their previous value.
- **Persistent config**: button mapping, pedal calibration and FFB settings in
  `app.json`, saved atomically.
- **Diagnostics**: `hid-probe` CLI for enumeration, raw report dumps, axis
  logging and torque tests.

## Supported hardware

| Device | VID | PID | Status |
|---|---|---|---|
| MOZA R3 wheelbase (+ R3 wheel, SR-P Lite pedals) | `0x346E` | `0x0005` | Verified |
| MOZA R3 (alternate PID) | `0x346E` | `0x0015` | Detected, not verified |

Other MOZA bases use the same protocol family and may work, but are untested.

## Requirements

- Windows 10/11 (x64)
- [ViGEm Bus Driver](https://github.com/nefarius/ViGEmBus/releases)
- Rust stable toolchain with the MSVC target, to build from source

MOZA Pit House may stay installed. Force feedback works with it running, but
its telemetry session shares the wheel's input report and can make the pedal
axes jitter. **Close Pit House for clean pedal input** (`hid-probe --axislog`
shows whether it is interfering).

## Download

Prebuilt Windows binaries are attached to every
[release](https://github.com/mstumberger/moza-xinput-bridge/releases/latest):

- `moza-xinput-bridge-<version>-windows-x64.zip`: the app, `hid-probe`,
  `pedal_log`, README, license and the FFB notes
- `moza-xinput-bridge-<version>.exe`: the app alone
- `SHA256SUMS.txt`: checksums

Builds are not code-signed, so Windows SmartScreen may warn on first launch.

## Build and run

```powershell
git clone https://github.com/mstumberger/moza-xinput-bridge.git
cd moza-xinput-bridge
cargo build --release
.\target\release\moza-xinput-bridge.exe
```

The app runs windowed with no console. It logs to `%TEMP%\moza_mapper.log`.

## Usage

1. Install the ViGEm Bus Driver and plug in the wheel.
2. Start the app and click **Start**: a virtual Xbox 360 controller appears.
3. **Force feedback**: enable it and set the gain (applied on the next Start).
4. **FFB test**: click **Open FFB test**, then hold **Push left** or
   **Push right**, or run **Auto pulse test** with your hands off the wheel.
   It reports how far the wheel moved for each pulse.
5. **Mapping wizard**: re-learn which wheel button drives each Xbox button.

### Default button mapping

Wheel button numbers are 1-based (Joystick Gremlin numbering).

| Xbox | Wheel | Xbox | Wheel |
|---|---|---|---|
| A | 1 | LB | 17 |
| B | 2 | RB | 18 |
| X | 3 | LS | 33 |
| Y | 4 | RS | 34 |
| D-pad U/R/D/L | 5/6/7/8 | Back | 35 |
| Guide | 38 | Start | 36 |

### Configuration

`app.json` lives next to the executable when that folder is writable, and in
`%APPDATA%\moza_mapper\` otherwise.

## How it works

### Input (42-byte report, ~860 Hz)

| Bytes | Content |
|---|---|
| 0 | Report ID `0x01` |
| 1-2 | Steering, u16 LE |
| 3-16 | Seven u16 axes, idle `0x8000`; throttle 5-6, brake 11-12, clutch 15-16 (all travel upward when pressed) |
| 17+ | Button bitmask from byte 17 bit 4 (bits 0-3 are status bits) |

### Force feedback

The R3 exposes a standard HID PID interface (interface `MI_02`). FFB reports
go over the interrupt-OUT pipe:

```
0C 08                    device control: reset
0C 01                    device control: enable actuators
0D FF                    device gain 100 %
05 01 00 00              set constant force, block 1, magnitude 0
01 01 01 FFFF ... 2823   set effect: block 1, constant force, infinite, direction 90.00 deg
0A 01 01 01              effect operation: start block 1
05 01 <i16 LE>           stream the torque (positive = turn left, negative = right)
```

Device Control is a **bitfield** and effect types are **1-based indices**.
Misreading those two points is why earlier attempts produced no force. The
full investigation, including the capture method, is in
[docs/FFB.md](docs/FFB.md).

## Diagnostics

```powershell
.\target\release\hid-probe.exe               # enumerate + stream decoded input
.\target\release\hid-probe.exe --raw         # feature-report scan + raw dump
.\target\release\hid-probe.exe --axislog     # log all axes (spot Pit House interference)
.\target\release\hid-probe.exe --torque-lib  # +-25 % torque pulses, measures movement (hands off!)
.\target\release\pedal_log.exe               # 90 s CSV log of the seven pedal axes
```

## Project layout

```
src/hid        device discovery, descriptor parsing, report decoding
src/config     persistent config, default R3 mapping
src/mapping    1-based button mapping + learning wizard
src/output     ViGEm X360 target, report builder, FFB notifications
src/ffb        HID PID force feedback: report encoding, Win32 transport, torque channel
src/gui.rs     egui UI: bridge control, live state, wizard, FFB settings and test
src/bin        hid-probe and pedal_log diagnostics
```

## Known limitations

- XInput only carries two rumble motor strengths, with no direction. In-game
  force through the virtual pad is therefore mapped from the large rumble
  motor and cannot reproduce directional steering forces. The torque channel
  itself is fully directional (see the FFB test panel).
- Only the R3 (PID `0x0005`) has been verified on hardware.

## Releasing

Create a GitHub release with a tag like `v0.4.0` targeting `main`. The
[release workflow](.github/workflows/release.yml) builds from the tagged
commit, attaches the zip, exe and checksums to the release, then pushes the
version bump in `Cargo.toml` back to `main`.

Dev builds: add the `build` label to a pull request, or run **Actions > CI >
Run workflow**. The build is uploaded as a workflow artifact named
`<version>-<commit>`. To package locally:

```powershell
cargo build --release
powershell -ExecutionPolicy Bypass -File tools\package.ps1 -Version 0.4.0
```

## Contributing

Issues and pull requests are welcome. For protocol work, a USBPcap capture
with phase markers is the most useful evidence: see the capture procedure in
[docs/FFB.md](docs/FFB.md). Please run `cargo test` before submitting.

## License

[MIT](LICENSE)
