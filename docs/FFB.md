# Force Feedback on the MOZA R3 — investigation log

Status: 2026-11-27. Input path: DONE + hardware-verified.
FFB: DONE, game protocol decoded and hardware-verified (see the 2026-11-27 section at the end).
The sections in between are the historical investigation; several conclusions in them turned out wrong.

## Verified protocol facts (live hardware, PID 0x0005, MI_02)

1. Caps: OutputReportByteLength = 51, FeatureReportByteLength = 18.
   Accepted output report IDs (SET_REPORT and interrupt-OUT): 0x01–0x06,
   0x0A–0x0D, 0x14–0x18.
2. USBPcap capture of the MOZA Windows driver (30 MB, wheelbase on hub 2):
   - FFB runs through the **MOZA Windows Driver** kernel stack
     ("MOZA WheelBase Virtual Device 2/3/4" = the DirectInput devices games
     see; native FFB goes: game → virtual device → kernel driver → wheel).
   - Driver's torque stream: 4-byte interrupt-OUT packets on endpoint 0x03,
     ~1.5 ms cadence: `05 02 <i16>` (63,763 packets), plus effect reports
     (`01 <block> <type> ...`, `03 01 ... <u16> <u16> ff7f 0000`, `0A <block>
     01 01|03 00`) and init (`0C 08`, `0D FF`, `0C 04`).
3. Direct torque registers (found empirically): **`0x14 <i16>`** and
   **`0x18 <i16>`** — a bare register write applies real force (32,000-count
   wheel swings measured).

## The wall

User-mode (us) can apply **exactly one torque episode per process**. After
that first applied burst, later writes (both transports, both registers,
with/without zero writes in between) are ACKed but ignored. In-process
handle reopen: dead. HID re-enumeration: dead. 20 s cooldown: dead. A fresh
process immediately after: armed again (reproduced 5+ times).

Since the wire bytes are identical, the arming state lives in something
process-external the MOZA kernel driver maintains — most likely the
feature-report handshake (0x12/0x13; 0x12 reads `02 01 FE FF` when the
driver session is live, `00 00 00 00` after wheel replug) or the 5-byte
status polls on endpoint 0x82.

Also: the spec-PID reports (Set Effect / Effect Operation / Set Constant)
are ACKed but never applied; PID `0C 01` (Reset) **wedges the base until
power-cycle** — never send it.

## Next step (needs the wheel + user)

Capture the ARMING handshake: start a USBPcap capture, then LAUNCH Pit
House (our existing capture began mid-session), drive 10 s with FFB, stop.
Diff the new capture's startup against our decoded set — whatever the
driver sends before its first `05 02` torque packet (feature SET_FEATURE to
0x12/0x13 is the prime suspect) is the missing arming step.

## Practical status for users

- Wheel-aware games: use the MOZA virtual device directly — FFB works
  natively (verified by capture); the bridge isn't needed for FFB there.
- XInput-only games: rumble-strength FFB; blocked on the arming handshake
  above.

## 2026-11-27: SOLVED, game protocol decoded and verified on hardware

Capture: `captures/ffb_20261127_183235_hub2.pcap` (local, gitignored), taken
from wheel plug-in through Pit House start and a real game session, with
phase markers in `captures/markers_20261127_183235.txt`. The capture includes
the R3's HID report descriptor, which settles the report semantics.

What the game (through the MOZA driver) actually sends, all as
**interrupt-OUT** reports to MI_02 (zero control transfers, zero feature
reports; the 0x11/0x12/0x13 "arming handshake" from 2026-08-26 is not part of
it):

| report | meaning (descriptor) | game bytes |
|---|---|---|
| `0C` | Device Control, **bitfield**: 01 enable, 02 disable, 04 stop all, 08 reset | `0C 08`, `0C 01` |
| `0D` | Device Gain (1 byte) | `0D FF` |
| `01` | Set Effect: block, type (1-based: 1 constant ... 9 damper), duration, ..., gain, trigger, `04`, direction 9000 | `01 01 01 FFFF 0000 0000 0000 FF FF 04 2823 00*6` |
| `0A` | Effect Operation: 1 start, 3 stop | `0A 01 01 01` / `0A 01 03 00` |
| `05` | Set Constant Force: block, i16 magnitude | `05 01 <i16>` about 19 Hz, followed by a Set Effect refresh |
| `03` | Set Condition (damper on block 2, coeff 0x2000) | `03 02 00 0000 0020 0020 FF7F FF7F 00 00` |
| `0B` | Block Free | `0B 01`, `0B 02` on exit |

Why every earlier attempt failed: `0C 04` was read as "enable actuators"
(it is Stop All Effects), `0C 01` was read as "reset" (it is Enable), the
torque effect was created as type 09 (Damper) and `05 02` constant-force
magnitudes were sent to that damper block. Report `0x14` is not used by the
game at all.

Verified live (`hid-probe --torque-lib`, Pit House running): +-25% pulses
moved the wheel 27k-40k counts, direction following the sign. **Positive
torque turns the wheel left** (steering raw value down), confirmed in the
GUI test.

### Reproducing a capture

Requires [USBPcap](https://desowin.org/usbpcap/) (bundled with the Wireshark
installer). From an elevated PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File tools\capture_ffb.ps1
```

The script captures every USB root hub to `captures\` and asks you to press
Enter after each phase (plug in, Pit House, game start, stationary wheel turn,
driving, hands off, quit). The phase timestamps go to `markers_*.txt`. Decode
with `tshark -r <file> -Y 'usb.dst=="<bus>.<dev>.3" && usb.data_len>0' -T fields -e frame.time_relative -e usbhid.data`.
