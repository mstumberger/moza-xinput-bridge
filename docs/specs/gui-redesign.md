# GUI redesign: design spec

Status: approved design, not yet implemented. Task breakdown:
[gui-redesign-tasks.md](gui-redesign-tasks.md). Later work: [phase-2.md](phase-2.md).

## 1. Goals

The current window is one long column of equally weighted boxes (FFB settings,
FFB test, shift lights, mapping wizard, live inputs, raw button grid). The main
job, start the bridge and see that it works, is buried between test and setup
tools.

The redesign must:

1. Make status and Start/Stop the most visible thing at all times.
2. Work for a first-time user who has never seen the app: plain labels, and a
   clear fix when the ViGEm driver or the wheel is missing.
3. Show live input on a drawn MOZA ES wheel whose buttons light up, with the
   shift lights on top, similar to the wheel page in Pit House.
4. Look deliberate: one dark theme, one accent colour, consistent spacing.

## 2. Decisions (settled with the owner)

| Topic | Decision |
|---|---|
| Toolkit | Stay on egui/eframe 0.31. No new UI framework. |
| Audience | First-time user of the public release. |
| Layout | Single dashboard, no page navigation. |
| Stopped state | Left column shows a status checklist and a large Start button. |
| Running state | Left column shows the drawn wheel and pedal bars. |
| Right column | Five collapsible sections, only one open at a time, last open one remembered. |
| Buttons | Shown on the drawn wheel. The raw numbered grid moves to Diagnostics. |
| Wheel art | Drawn in code from our own geometry. No MOZA images or artwork in the repo. |
| Wheel behaviour | Rotates with steering; button labels stay upright. Replaces the old steering bar and button row. |
| Theme | Dark only. Accent: lime green. |
| Language | English only. |
| Window | Resizable, default 960x640, minimum 820x560. |
| Scope | Presentation plus splitting `src/gui.rs` into modules. Bridge, FFB, lights and mapping behaviour do not change. |
| Delivery | One branch (`feat/gui-redesign`), tasks done in order, one PR. |

## 3. Scope

In scope:

- New layout, theme and widgets as described here.
- Split `src/gui.rs` (about 1060 lines) into the module tree in section 9.
- Small read-only additions that the new views need (section 8). They must not
  change what the bridge, FFB, lights or wizard do.

Out of scope (see [phase-2.md](phase-2.md)): rumble as vibration, tray icon,
start with Windows, settings that apply while running, pedal calibration UI,
a click-to-place button layout editor, translations, a light theme.

## 4. Window layout

```
+--------------------------------------------------------------------------+
| MOZA XInput Bridge   [Wheel] [Driver] [FFB] [Lights]   860 Hz    [ STOP ] |  top bar, 56 px
+--------------------------------------------------------------------------+
| (error banner, only when there is an error)                               |
+----------------------------------------------+---------------------------+
|                                              | v Force feedback          |
|              LEFT COLUMN                     |   ...open section body... |
|   stopped: checklist + big Start             | > FFB test                |
|   running: wheel + pedal bars                | > Shift lights            |
|                                              | > Button mapping          |
|   grows with the window                      | > Diagnostics             |
|                                              |   fixed width 340 px      |
+----------------------------------------------+---------------------------+
```

- Top bar: `TopBottomPanel::top`, height 56.
- Right column: `SidePanel::right`, exact width 340, not resizable, vertical
  `ScrollArea` as a safety net only.
- Left column: `CentralPanel`.
- Window: default inner size 960x640, minimum 820x560, title
  `MOZA XInput Bridge`.

### 4.1 Top bar

Left to right:

1. App name `MOZA XInput Bridge`, heading size, primary text colour.
2. Four status pills (section 6.3):
   - **Wheel**: `Connected` (good) when the bridge has the wheel open;
     `Detected` (neutral) when stopped and the wheel is present on USB;
     `Not found` (bad) otherwise.
   - **Driver**: ViGEm bus. `Ready` (good) / `Missing` (bad) / `Checking` (neutral).
   - **FFB**: `Active` (good) when `ffb_active`; `Off` (neutral) when disabled
     in config; `Standby` (neutral) when enabled but the bridge is stopped;
     `Failed` (bad) when enabled, running, and not active.
   - **Lights**: `On` (good) when the lights controller is running; `Off`
     (neutral); `Error` (bad) when `lights_error` is set.
3. Report rate, e.g. `860 Hz`, muted text, only while running.
4. Right-aligned Start/Stop button, 110x36:
   - Stopped: label `Start`, accent fill, dark text.
   - Running: label `Stop`, red fill, light text.
   - Disabled (wizard running): grey, tooltip
     `Finish or cancel the mapping wizard first`.

### 4.2 Error banner

Directly under the top bar, only when `config_error` or `last_error` is set.
Red-tinted background, one line each, with a dismiss `x` for `last_error`
(config errors stay until fixed, as now). Text wraps.

### 4.3 Left column, stopped

Centered block, max width 460:

1. Title `Ready to drive?` (or `Almost there` when any required item fails).
2. Checklist rows (icon, title, one-line detail, optional action):

| Item | Good | Bad / warning | Action |
|---|---|---|---|
| ViGEm Bus Driver (required) | `Installed` | `Not installed. The virtual Xbox controller needs it.` | Button `Get the driver` opens `https://github.com/nefarius/ViGEmBus/releases` |
| MOZA wheel (required) | `Connected over USB` | `Not found. Plug in the wheelbase and switch it on.` | none |
| MOZA Pit House (advisory) | `Not running` | Warning: `Running. Pedals may jitter while it is open.` | none |

3. Large Start button, 220x52, accent fill. Disabled with a muted hint
   `Fix the items above first` while a required item is bad. The top-bar Start
   stays enabled regardless (the bridge reports its own errors), so a wrong
   probe can never lock the user out.
4. Muted footer: `Starts a virtual Xbox 360 controller driven by your wheel.`

### 4.4 Left column, running

- Wheel widget (section 7), centered, as large as fits: side length =
  `min(available_width - 150, available_height - 24)`, clamped to 240..640.
- Pedal bars to the right of the wheel: three vertical bars, each 28 wide,
  height 60 % of the wheel side, 14 gap. Labels below: `THR`, `BRK`, `CLU`,
  value above as a whole percent. Fill colours: throttle = accent, brake =
  red, clutch = blue. Track = surface colour.
- Below the wheel, one muted line: `Steering +12.4 %` and, if any are pressed,
  `Buttons 3, 17`.

When the window is narrower than 900, the pedal bars move below the wheel as
three horizontal bars.

## 5. Right column sections

Implemented as a custom accordion: a header row (chevron, title, optional
status text on the right) and a body. Clicking a closed header opens it and
closes the others; clicking the open header closes it. The open section id is
stored in `config.ui.open_section` and saved on change.

Section ids: `ffb`, `ffb_test`, `lights`, `mapping`, `diagnostics`.
Default open: `ffb`.

All controls keep their current behaviour and save rules. Only labels, grouping
and appearance change.

### 5.1 Force feedback

Header status: `On` / `Off`.

- Toggle `Enable force feedback`.
- Slider `Gain`, 0.1 to 2.0, shown as a percentage (`100 %` = 1.0).
- Muted note: `Changes apply the next time you press Start.`
- Muted note: `Games send only rumble to an Xbox controller, so in-game force
  through the bridge is approximate.`

### 5.2 FFB test

Header status: `Open` when the test channel is open.

Closed state: one paragraph
`Push the wheel directly to check the motor. Stop the bridge first for a clean
reading.` and a button `Open FFB test`.

Open state:

- Slider `Strength`, 5 to 100 %.
- Two hold buttons side by side, each 150x40: `Push left`, `Push right`.
  Force applies only while held (existing behaviour and sign).
- Mini steering bar: thin horizontal bar with a centre mark and the offset in
  percent.
- Button `Auto pulse test`, with the note `Hands off the wheel.`
- Result card, coloured by verdict: good for `OK`, bad for `NO FORCE` and
  `REVERSED`, warning otherwise. First line is the verdict, the count lines
  are muted.
- Button `Close FFB test`.
- Errors (`ffb_test_error`) show as a dismissible bad-coloured line at the top.

### 5.3 Shift lights

Header status: `On` / `Off`.

- Toggle `Enable shift lights` with note `Uses the wheelbase serial port.`
- When enabled:
  - `Max RPM` drag value, 500 to 20000.
  - `LEDs` slider 1 to 16.
  - `First light at` slider 0 to 99 %, `All lit at` slider 1 to 100 %
    (existing clamping rules).
  - Sub-heading `Sources`:
    - Toggle `UDP telemetry`, port drag value 1024 to 65535, note
      `Turn lights off and on to apply a new port.`
    - Toggle `Controller rumble`.
  - Sub-heading `Test`:
    - Slider `Manual RPM`, 0 to max.
    - Button `Test sweep` / `Stop sweep`.
- Errors (`lights_error`) as in 5.2.

### 5.4 Button mapping

- Idle: a two-column table of the 15 Xbox buttons and their wheel button
  number (`A  1`, `B  2`, ...; `0` shown as `not set`). Display names:
  `DPAD_UP` as `D-pad up`, `LB` as `LB`, `Guide` as `Guide`, etc.
- Button `Re-learn mapping`. Disabled with tooltip `Stop the bridge first`
  while the bridge runs (the wizard already refuses to start then).
- Wizard running: accent-coloured prompt line (`Press the wheel button for:
  A`), the last five log lines in monospace, and `Cancel (keep what was
  learned)`.

### 5.5 Diagnostics

- Raw button grid: buttons 1 to 38, ten per row, 26x24 cells, lit in accent
  when pressed. Line above: `Pressed: 3, 17` or `No buttons pressed`.
- `Steering range` drag value, 180 to 2520 degrees, default 900, step 10.
  Stored in `config.ui.steering_range_deg`. Only affects how far the drawn
  wheel rotates.
- Stats: report rate, total reports.
- Button `Open log file` (opens `%TEMP%\moza_mapper.log` with the default
  app) and button `Open config folder`.
- App version from `CARGO_PKG_VERSION`, and a link `Project page` to
  `https://github.com/mstumberger/moza-xinput-bridge`.

## 6. Theme

All colours and sizes live in `src/gui/theme.rs`. No colour literals anywhere
else in `src/gui/`.

### 6.1 Colours

| Token | Hex | Use |
|---|---|---|
| `BG` | `#0F1113` | window background |
| `PANEL` | `#16191C` | top bar, right column |
| `SURFACE` | `#1E2226` | cards, section bodies, bar tracks |
| `SURFACE_HOVER` | `#262B30` | hovered rows and buttons |
| `BORDER` | `#2E343A` | 1 px separators and outlines |
| `TEXT` | `#E6E8EA` | primary text |
| `TEXT_MUTED` | `#8B939B` | notes, secondary values |
| `ACCENT` | `#A3E635` | lime: primary action, active/pressed, good status |
| `ACCENT_TEXT` | `#0F1113` | text on accent fills |
| `WARN` | `#F5B83D` | warnings |
| `BAD` | `#F0524F` | errors, Stop button, brake |
| `INFO` | `#4BA3F5` | clutch bar |
| `WHEEL_RIM` | `#2A2F34` | drawn wheel rim |
| `WHEEL_HUB` | `#1A1D20` | drawn wheel hub plate |

Lime doubles as the "good" colour. No status may rely on colour alone: every
pill and checklist row also has an icon and a word.

### 6.2 Type and spacing

- Font: egui default proportional and monospace. No bundled fonts.
- Sizes: heading 20, section title 15, body 14, small/muted 12, pill 12.
- Spacing unit 8: item spacing (8, 8), section padding 12, column padding 16.
- Corner radius: 6 for cards and buttons, 999 (pill) for status pills.
- Interactive minimum height 28.

### 6.3 Shared widgets (`src/gui/widgets/`)

- `status_pill(ui, label, text, Level)` where `Level` is `Good | Neutral |
  Warn | Bad`. Shape: small filled dot plus `label: text`, on `SURFACE`, pill
  radius. Dot and text colour follow the level (neutral = `TEXT_MUTED`).
- `toggle(ui, &mut bool, label)`: switch-style toggle, accent when on.
- `section(ui, id, title, status, open_id, body)`: the accordion item.
- `note(ui, text)`: muted small wrapped text.
- `result_card(ui, Level, title, lines)`.
- `checklist_row(ui, Level, title, detail, action)`.

Icons are drawn with the painter (dot, check, cross, warning triangle,
chevron). No icon font, no image files.

## 7. Wheel widget

`src/gui/widgets/wheel.rs`. Pure painter drawing inside a square of side `s`,
centre `c`, unit radius `R = s / 2`. All geometry is in **normalized wheel
units**: x right, y down, the rim outer edge at distance 1.0 from the centre.

Input:

```rust
pub struct WheelView<'a> {
    pub steer: f64,                 // -1.0 ..= 1.0
    pub steering_range_deg: f32,    // full lock-to-lock, e.g. 900
    pub buttons: &'a [bool; 64],    // index N-1 = wheel button N
    pub labels: &'a [Option<&'a str>; 64], // short Xbox name per wheel button, if mapped
    pub led_mask: Option<u16>,      // None = lights off
    pub led_count: u8,              // 1..=16
}
```

### 7.1 Rotation

`angle = steer * (steering_range_deg / 2)` degrees, clockwise for positive
steer. Every wheel-space point is rotated about the centre by `angle`, then
scaled by `R` and offset by `c`. Text is never rotated: labels are drawn
upright at the rotated position.

Provide a pure function and unit-test it:

```rust
pub fn to_screen(p: Vec2, angle_rad: f32, center: Pos2, r: f32) -> Pos2
```

### 7.2 Geometry (normalized units)

- Rim: ring, outer radius 1.00, inner radius 0.80, colour `WHEEL_RIM`.
  Drawn as a circle stroke of width `0.20 * R` at radius 0.90.
- Top centre marker: short radial stripe at 12 o'clock, from radius 0.81 to
  0.99, width 0.05, colour `TEXT`. It makes rotation readable at a glance.
- Hub plate, colour `WHEEL_HUB`, outline `BORDER`. Convex pieces only (egui
  fills convex polygons):
  - Upper plate, polygon: (-0.56,-0.20) (-0.30,-0.27) (0.30,-0.27)
    (0.56,-0.20) (0.60,0.16) (-0.60,0.16).
  - Lower plate, polygon: (-0.60,0.16) (0.60,0.16) (0.50,0.36) (0.14,0.60)
    (-0.14,0.60) (-0.50,0.36).
  - Bottom spoke: polygon (-0.14,0.58) (0.14,0.58) (0.20,0.82) (-0.20,0.82).
  - Side spokes: polygons joining the plate to the rim,
    left (-0.60,-0.06) (-0.82,-0.10) (-0.82,0.10) (-0.60,0.14), right mirrored.
- Centre badge: hexagon radius 0.12 at (0, 0.02), colour `SURFACE`, no text.
- Paddles: one paddle per side, drawn as two rounded rectangles each (the
  part visible above the side spoke and the part below it), colour
  `SURFACE`: upper parts centred (-0.66,-0.22) and (0.66,-0.22), size
  0.12 x 0.26; lower parts centred (-0.62,0.34) and (0.62,0.34), size
  0.12 x 0.30. Button 13 lights both left parts, button 14 both right parts.

### 7.3 Button layout

Positions measured from Pit House's ES wheel page; numbers are the 1-based
wheel button numbers used everywhere in this app.

| Button | x | y | Shape | Notes |
|---|---|---|---|---|
| 1 | 0.42 | 0.12 | round | right cluster, bottom (A) |
| 2 | 0.50 | 0.04 | round | right cluster, right (B) |
| 3 | 0.34 | 0.04 | round | right cluster, left (X) |
| 4 | 0.42 | -0.04 | round | right cluster, top (Y) |
| 5 | -0.43 | -0.04 | round | D-pad up |
| 6 | -0.35 | 0.04 | round | D-pad right |
| 7 | -0.43 | 0.12 | round | D-pad down |
| 8 | -0.51 | 0.04 | round | D-pad left |
| 13 | -0.66 | -0.22 | paddle | left paddle (LB), verified on hardware |
| 14 | 0.66 | -0.22 | paddle | right paddle (RB), verified on hardware |
| 19 | -0.49 | -0.14 | round | |
| 20 | -0.39 | -0.19 | round | |
| 21 | -0.23 | -0.11 | round | |
| 22 | -0.49 | 0.26 | round | |
| 23 | -0.29 | 0.39 | large | left knob |
| 24 | -0.07 | 0.34 | small | |
| 25 | 0.00 | 0.43 | small | |
| 32 | 0.48 | -0.14 | round | |
| 33 | 0.37 | -0.19 | round | |
| 34 | 0.22 | -0.11 | round | |
| 35 | 0.48 | 0.26 | round | |
| 36 | 0.28 | 0.39 | large | right knob |
| 37 | 0.06 | 0.34 | small | |
| 38 | 0.00 | 0.52 | small | |

Radii: round 0.040, small 0.030, large 0.075. Paddle buttons light the whole
paddle shape instead of drawing a circle.

The table is a `const` array in `wheel.rs`:

```rust
pub struct ButtonSpot { pub button: u8, pub x: f32, pub y: f32, pub kind: SpotKind }
pub const ES_WHEEL_LAYOUT: &[ButtonSpot] = &[ /* table above */ ];
```

Unit tests: no duplicate button numbers; every spot lies inside the rim
(`x*x + y*y < 0.80*0.80`) except paddles; all button numbers are in 1..=64.

### 7.4 Button states

- Idle: fill `SURFACE`, 1 px `BORDER` outline.
- Pressed: fill `ACCENT`, plus a soft glow (a second circle, radius x1.6,
  accent at 25 % alpha).
- Label: if `labels[n-1]` is set, draw it centred on the spot, size
  `max(9, 0.045 * R)`, colour `TEXT_MUTED` idle and `ACCENT_TEXT` pressed.
  Labels longer than 2 characters are drawn just below the spot instead of
  inside it. Short names: `A B X Y`, D-pad arrows as `▲ ▶ ▼ ◀`, `LB RB LS RS`,
  `Back`, `Start`, `Guide`.

### 7.5 Buttons with no position

Any wheel button that is pressed but has no entry in `ES_WHEEL_LAYOUT` (for
example 9 to 12, 15 to 18, 26 to 31) appears as a chip in a row under the
wheel: `17`, or `17 LB` when it is mapped, lit in accent. This row is not rotated. It guarantees every
press is visible even if the layout table is incomplete.

### 7.6 Shift lights on the wheel

A strip of `led_count` circles on the hub, centred at y = -0.23, spread evenly
from x = -0.17 to 0.17, radius `min(0.022, 0.17 / led_count)`.

- `led_mask == None`: strip drawn in `SURFACE` (lights off).
- Bit i set: LED i lit. Colour by position: first 40 % `ACCENT`, next 40 %
  `WARN`, last 20 % `BAD`.

## 8. Data and state additions

These are the only non-presentation changes. All are additive.

1. `config::UiConfig`, added to `AppConfig` with `#[serde(default)]` so old
   `app.json` files load unchanged:

```rust
pub struct UiConfig {
    pub open_section: String,        // default "ffb"; "" = all closed
    pub steering_range_deg: u16,     // default 900
}
```

2. `rpm::LightsHandle::current_mask() -> u16`: the last LED mask sent, kept in
   an `AtomicU16` updated by the writer thread. Read-only for the GUI.

3. `gui::probe`: a background thread that runs **only while the bridge is
   stopped**, every 2 seconds, and publishes:
   - `vigem: Option<bool>` via `vigem_rust::Client::connect()` succeeding
     (the client is dropped immediately; no target is created),
   - `wheel: bool` via a hidapi device list refresh matching `MOZA_VID` and
     `MOZA_PIDS` (enumeration only; the device is never opened),
   - `pit_house: bool` via a process list check for `MOZA Pit House.exe`
     (`CreateToolhelp32Snapshot`, add the `Win32_System_Diagnostics_ToolHelp`
     feature to `windows-sys`).

   Results sit in an `Arc<Mutex<ProbeState>>`. The thread must stop within
   250 ms of the bridge starting or the app closing.

## 9. Module layout

`src/gui.rs` becomes a directory. Public API stays `gui::MozaMapperApp` and
`gui::AppState`.

```
src/gui/
  mod.rs            MozaMapperApp struct, new(), eframe::App::update (layout only), Drop
  state.rs          AppState
  bridge.rs         run_bridge, start_bridge, stop_bridge
  ffb_test.rs       FfbTest, ffb_pulse, start_ffb_test, stop_ffb_test
  lights.rs         start_lights, stop_lights, start_sweep, stop_sweep, UDP listener glue
  wizard.rs         start_wizard, wizard_running
  probe.rs          environment probe thread (section 8.3)
  theme.rs          colours, sizes, apply(ctx)
  widgets/
    mod.rs          status_pill, toggle, section, note, result_card, checklist_row, icons
    wheel.rs        wheel widget and ES_WHEEL_LAYOUT
    pedals.rs       pedal bars
  panels/
    top_bar.rs
    left_stopped.rs checklist view
    left_running.rs wheel + pedals view
    ffb.rs
    ffb_test.rs
    lights.rs
    mapping.rs
    diagnostics.rs
```

Each `panels/*.rs` exposes one function `pub fn show(app: &mut MozaMapperApp,
ui: &mut egui::Ui)` (plus `ctx` where needed). `mod.rs::update` only arranges
panels and calls these functions.

## 10. Behaviour that must not change

- Start/Stop semantics, the running-flag reconciliation, and the neutral
  report on stop.
- The wizard and the bridge never read the wheel at the same time.
- Every config control saves on change through `AppConfig::save()` and reports
  failures in `config_error`.
- FFB test torque sign (positive = left), cadence (5 ms active, 50 ms idle)
  and the auto pulse verdict rules.
- Shift-light clamping rules and live `set_config`.
- Shutdown order in `Drop`: wizard cancel and join, FFB test, lights, bridge.
- Repaint: keep `request_repaint_after(16 ms)`.

## 11. Acceptance criteria

1. `cargo build --release` with `RUSTFLAGS=-D warnings` succeeds.
2. `cargo test` passes, including new tests for wheel geometry, the layout
   table, `UiConfig` defaults and old-config loading.
3. No colour literal outside `theme.rs`; no file in `src/gui/` over 400 lines.
4. With no wheel plugged in, the app opens to the checklist and names the
   missing item; with the ViGEm driver missing it offers the download link.
5. Pressing Start with the wheel connected shows the wheel view; turning the
   wheel rotates the drawing the same way; pressing A, B, X, Y and the D-pad
   lights the matching spots; a button with no position appears as a chip.
6. With shift lights enabled, the manual RPM slider lights the strip on the
   drawn wheel in step with the real LEDs.
7. Opening one right-column section closes the others, and the open section is
   restored after restarting the app.
8. At 820x560 nothing overlaps or is cut off; at 1600x1000 the wheel grows and
   stays centred.
9. All existing functions (FFB test, auto pulse test, lights test sweep,
   mapping wizard) work exactly as before.

## 12. Visual checks during implementation

`tools/capture_window.ps1` captures one window to a PNG by process name:

```powershell
powershell -ExecutionPolicy Bypass -File tools\capture_window.ps1 -ProcName moza-xinput-bridge -Out shot.png
```

Each UI task ends with a capture of the running app, checked against this
spec. The owner reviews the finished app once, at the end.
