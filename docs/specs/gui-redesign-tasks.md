# GUI redesign: task breakdown

Implements [gui-redesign.md](gui-redesign.md) ("the spec"). Tasks run **in
order**, one worker at a time, all on branch `feat/gui-redesign`. Each task is
one commit.

## Rules for every task

1. Read the spec sections named in the task before writing code. The spec
   wins over this file if they disagree; report the conflict.
2. Stay inside the task. Do not start later tasks, do not refactor code the
   task does not name, do not change behaviour listed in spec section 10.
3. Before committing, all of these must pass:
   - `cargo build` with no warnings,
   - `cargo test`,
   - for UI tasks: run the app, capture it with
     `tools\capture_window.ps1 -ProcName moza-xinput-bridge -Out <file>.png`,
     look at the capture, and fix what does not match the spec. Close the app
     afterwards (`taskkill /IM moza-xinput-bridge.exe /F`).
4. The hardware may or may not be plugged in. Never send torque or run the FFB
   test, the auto pulse test or the lights sweep from a worker. Visual checks
   of the running view use the debug preview from Task 5.
5. Commit message: one imperative summary line, a short body listing what
   changed. No co-author or AI attribution lines. Do not push.
6. Final report: what was done, test and build results, the capture paths,
   and anything that deviates from the spec and why.

---

## Task 1: split `src/gui.rs` into modules (no visible change)

Spec: section 9, section 10.

- Turn `src/gui.rs` into the `src/gui/` tree from section 9, creating only the
  files that have existing code to hold: `mod.rs`, `state.rs`, `bridge.rs`,
  `ffb_test.rs`, `lights.rs`, `wizard.rs`, and `panels/` files for the code
  currently inline in `update` (top bar, FFB, FFB test, lights, mapping,
  diagnostics = the raw grid and live bars for now).
- Move code verbatim. Fields that panels need become `pub(crate)`.
- `update` in `mod.rs` keeps the current single-column order and look.

Done when: the app looks and behaves exactly as before, no file in `src/gui/`
is over 400 lines, build and tests pass. Capture: `shots/t1.png`.

## Task 2: theme and shared widgets

Spec: section 6.

- Add `theme.rs` with every colour token and size from 6.1 and 6.2, and
  `theme::apply(ctx)` that sets egui `Visuals` and `Style` (dark base, panel
  and widget fills, rounding, spacing, text sizes, selection = accent).
  Call it once from `MozaMapperApp::new`.
- Add `widgets/mod.rs` with `status_pill`, `toggle`, `note`, `result_card`,
  `checklist_row`, the painter icons, and `Level`.
- Replace every `egui::Color32::` literal in `src/gui/` with theme tokens.
- `main.rs`: window title `MOZA XInput Bridge`, default 960x640, minimum
  820x560.

Done when: no colour literal outside `theme.rs` (`grep -rn "Color32::" src/gui`
shows only `theme.rs` and `from_rgb` style helpers there), the old layout
renders in the new theme. Capture: `shots/t2.png`.

## Task 3: dashboard shell, top bar and accordion

Spec: section 4 (4.1, 4.2), section 5 intro, section 8.1.

- Add `config::UiConfig` exactly as in 8.1, with tests: defaults, and an old
  `app.json` without `ui` loads with defaults.
- `update`: top bar panel, error banner, right `SidePanel` (340) with the
  accordion, `CentralPanel` for the left column.
- Add `widgets::section` (one open at a time, persisted to
  `config.ui.open_section`, saved on change).
- Move the existing section bodies into the accordion unchanged (restyling is
  Task 6). Left column temporarily shows the old live bars.
- Top bar per 4.1 with the four pills. Until Task 4, `Driver` shows
  `Checking` and `Wheel` uses only `device_connected`.

Done when: acceptance criterion 7 holds, Start/Stop works from the top bar,
layout holds at 820x560 and 1600x1000. Captures: `shots/t3-small.png`,
`shots/t3-large.png`.

## Task 4: environment probe and the stopped view

Spec: 4.3, 8.3, 4.1 (final pill rules).

- Add `probe.rs` per 8.3, including the `windows-sys` feature. Start the
  probe when the app opens and whenever the bridge stops; stop it when the
  bridge starts and in `Drop`.
- Add `panels/left_stopped.rs` with the checklist and the large Start button.
- Wire the `Wheel` and `Driver` pills to the probe results.
- Unit-test the pure parts: mapping probe results to checklist rows and to
  pill levels.

Done when: acceptance criterion 4 holds on a machine state you can reach
(report which states you could verify), and the probe thread provably stops
(log line on exit). Capture: `shots/t4.png`.

## Task 5: wheel widget and pedal bars

Spec: section 7, 4.4, 8.2.

- Add `rpm::LightsHandle::current_mask()` per 8.2 with a unit test.
- Add `widgets/wheel.rs`: `WheelView`, `to_screen`, `ES_WHEEL_LAYOUT`, the
  geometry in 7.2, states in 7.4, chips in 7.5, LED strip in 7.6.
- Add `widgets/pedals.rs` and `panels/left_running.rs` per 4.4, including the
  narrow-window fallback.
- Build the `labels` array from `config.mapping` (short names from 7.4).
- Unit tests per 7.1 and 7.3.
- Debug preview: when the environment variable `MOZA_GUI_PREVIEW=1` is set,
  the left column shows the running view fed by a synthetic `DeviceState`
  (steer sweeping slowly, pedals moving, buttons 1, 6, 13, 17, 36 pressed in
  turn, LED mask counting up) without opening any device. This is how workers
  and reviewers check the wheel without hardware. It must not affect normal
  runs.

Done when: tests pass and the preview shows rotation, lit buttons with
labels (LB on the left paddle), a chip for button 17, pedal bars and the LED strip. Captures:
`shots/t5-a.png`, `shots/t5-b.png` taken about 2 seconds apart, and
`shots/t5-narrow.png` at 820x560.

## Task 6: restyle the five sections

Spec: 5.1 to 5.5.

- Rework each `panels/*.rs` body to the labels, grouping and widgets in the
  spec, using `toggle`, `note`, `result_card`.
- Mapping section: the two-column table and the disabled state while running.
- Diagnostics: raw grid, steering range control, stats, `Open log file`,
  `Open config folder`, version and project link. Opening files and URLs uses
  `ctx.open_url` for links and `explorer.exe` for paths.
- Remove the temporary live bars from the left column.

Done when: every control listed in section 5 exists with its exact label and
still saves on change; acceptance criterion 9 holds for everything that does
not need hardware (report what you could not exercise). Captures: one per
open section, `shots/t6-<id>.png`.

## Task 7: polish, docs, final checks

Spec: section 11.

- Go through all acceptance criteria and fix gaps. Check both window sizes.
- Repaint throttle while minimised (spec section 10).
- Wheel drawing polish: hub and spokes need more contrast against the
  background and the rim.
- Remove dead code and unused theme tokens; keep `MOZA_GUI_PREVIEW`.
- README: replace the usage steps that describe the old layout, add a
  screenshot `docs/img/dashboard.png` (the preview running view) and mention
  the wheel view and checklist in Features.
- Add `shots/` to `.gitignore` and delete the working captures.
- `cargo build --release` with `RUSTFLAGS=-D warnings`, `cargo test`.

Done when: all criteria that do not need hardware are met, and the report
lists the criteria that still need the owner's hardware check (5, 6, 9).

---

## After the tasks

The reviewer checks the diff and captures against the spec, fixes or sends
back anything off, then the owner tests on hardware (criteria 5, 6 and 9) and
the branch goes up as one pull request.
