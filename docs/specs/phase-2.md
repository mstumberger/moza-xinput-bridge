# Phase 2: further improvements

Not part of the GUI redesign. Each item is independent and can ship as its own
pull request, in the suggested order below.

## 1. Rumble as vibration

**Problem.** Games send the virtual Xbox controller two rumble strengths with
no direction. Today the large motor value becomes a constant force, which
pushes the wheel one way for as long as the game rumbles.

**Change.** Play rumble as a periodic effect instead of a constant one:

- Create a Sine effect (effect type 4) on a second block next to the existing
  constant block: Set Effect, then Set Periodic (report `04`: block,
  magnitude, offset, phase, period).
- Large motor: low frequency, about 25 Hz (period 40 ms), magnitude from the
  motor value. Small motor: higher frequency, about 80 Hz, on a third block.
- Gain slider scales both. Zero rumble stops the effects.
- Config option `ffb_mode`: `vibration` (new default) or `constant` (current).

**Needs first.** A short USB capture or probe run confirming the R3 accepts
Set Periodic with these values; the report layout is in the HID descriptor
but has not been exercised. Add a `hid-probe --rumble-test` mode that plays
each motor for one second.

**Done when.** A game's rumble is felt as vibration with no net steering
push, verified with the steering readout staying within 1 % of its start.

## 2. Settings apply while running

**Problem.** FFB enable and gain say "applies on next Start".

**Change.** `FfbController::set_gain` already exists; call it from the gain
slider. For enable/disable, keep the controller alive and add
`set_enabled(bool)` that zeroes the force and stops or starts the effect.
Remove the "next Start" notes. Shift-light UDP port: restart the listener in
place instead of asking for an off/on cycle.

**Done when.** Moving the gain slider during a rumble changes the force
immediately, and no control in the GUI needs a restart to take effect.

## 3. Tray icon and start with Windows

**Change.**

- Closing the main window sends the app to the system tray (crate
  `tray-icon`) instead of quitting. The bridge, force feedback and shift
  lights keep running. Tray menu: Show, Start/Stop bridge, Quit. The first
  close shows a one-time hint that the app is still running in the tray.
- **No rendering while in the tray.** The window is hidden, not just
  minimised, and the GUI does no work: no repaint requests, no wheel
  animation, no per-frame polling of bridge state. Only the worker threads
  (bridge, FFB, lights, UDP listener) run. Anything the GUI polled per frame
  must not be needed for correctness while hidden (for example the wizard
  result pickup and the bridge-thread reconciliation run again on the first
  frame after the window is shown). Target: under 0.5 % CPU from the GUI
  thread while in the tray.
- The environment probe (checklist) does not run while the window is hidden.
- Options in a new `General` section: `Start with Windows` (per-user Run
  registry key), `Start minimised`, `Start the bridge automatically`.
- Auto-start waits for the wheel: if it is not present, keep probing and start
  the bridge when it appears.

**Risks.** An earlier C# prototype had a tray icon that made the Windows
taskbar auto-hide misbehave; test with taskbar auto-hide on. Requires an app
icon (draw our own; no MOZA logo).

**Done when.** After a reboot with the options on, the virtual controller is
available without touching the app, the tray menu controls it, closing the
window keeps the bridge running, and the app uses no measurable GUI CPU while
in the tray.

## 4. Pedal calibration UI

**Problem.** Pedal ranges and directions live only in `app.json`.

**Change.** A `Pedals` section: for each pedal a live raw bar, `Set released`
and `Set fully pressed` buttons that capture the current raw value, a
direction toggle, and a dead-zone slider at each end. A `Calibrate all`
flow: press each pedal fully once, release, done. Values save on change and
apply to the running bridge.

**Done when.** A fresh install can be calibrated without editing a file, and
each pedal reads 0 % released and 100 % pressed.

## 5. Button layout editor

**Problem.** The drawn wheel's button positions are fixed for the ES wheel.
Buttons 9 to 12, 15 to 18 and 26 to 31 have no position and show as chips.

**Change.** A `Place buttons` mode in Diagnostics: press a wheel button, then
click where it sits on the drawing. Positions are stored in `app.json` as an
override of `ES_WHEEL_LAYOUT`, with `Reset to default`. Lets other MOZA wheels
be supported without code changes.

## 6. Smaller items

- UDP telemetry listener binds `127.0.0.1` by default, with an option to
  listen on the network.
- Code-sign release builds if a certificate becomes available.
- Wheel presets for other MOZA rims (layout table per wheel).
- Translations (Slovenian first).
