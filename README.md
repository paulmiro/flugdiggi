# flugdiggi

Minimal Linux client for the Flydigi Vader 4 Pro (old "XInput" protocol of Flydigi Space Station),
reverse engineered from Space Station 4.2.0.9. Tested with the 2.4 GHz dongle and over USB cable.

The dongle enumerates as an Xbox 360 pad (`045e:028e`, manufacturer "Flydigi"). flugdiggi talks to
its vendor interface via libusb; `xpad` is detached while it runs and reattached afterwards. No
driver, no root (if the device node is accessible), the gamepad is unusable only while a command runs.

## Build

```sh
nix-shell --run 'cargo build --release'
```

## Usage

```
flugdiggi [-v] <command>

  info                        device info, active slot, slot versions
  slot [N]                    show or switch the active slot (0-3)
  export <slot> [file.toml]   print or save a slot's config as TOML
  import <slot> <file.toml>   write a (modified) TOML config to a slot
  rename <slot> <name>        rename a slot's config
  dump <slot> <file.bin>      save a slot's raw config blob
  restore <slot> <file.bin>   write a raw config blob back to a slot
  decode <file.bin>           print a raw config blob as TOML (offline)
  led-export <slot> [file]    print or save a slot's LED config as TOML
  led-import <slot> <file>    write a (modified) TOML LED config to a slot
  led-dump <slot> <file.bin>  save a slot's raw LED blob
  led-restore <slot> <file>   write a raw LED blob back to a slot
  raw <cmd[:arg...]>...       send read-only commands and print all replies
```

Typical edit: `flugdiggi export 1 cfg.toml`, edit, `flugdiggi import 1 cfg.toml`.

- Slots 0-3 are the XInput-mode configs (writable), 4-7 the Switch-mode configs (read-only for now).
- Every write first saves the slot's previous content to
  `flugdiggi-backup-<kind>-slot<N>-<time>.bin` in the current directory, then saves to flash,
  reloads the slot and verifies both its mapping and LED config. A write takes 15-35 s.
- `info` reports the battery as a level 0-5 (6 = charging).
- The LED shown is the active slot's LED config.

## LED config (`led-export` / `led-import`)

The Vader 4 Pro has 5 LEDs in a V shape. Each has 10 animation steps (`[leds]`, colours `#rrggbb`).

- `loop_end`: the animation runs over steps `0..=loop_end` and fades smoothly between steps (and from the
  last step back to step 0). `loop_end = 0` gives a static colour from step 0.
- `step_time`: duration of one step, about 100 ms per unit (`15` = 1.5 s per step).
- `feedback = 1`: the LEDs react to buttons instead of looping. Every button press (incl. stick clicks)
  plays steps `0..=loop_end` once and the last step stays; e.g. colour at step 0 and black at step 1 gives a
  flash that fades out.
- `brightness`: 1-100, has only a weak visible effect.
- `app_preset`, `loop_start`, `led_count`, `unused_leds` are ignored by the firmware and only kept as-is.
  `app_preset` is what the official app shows as selected (1 flowing, 2 breathing, 3 gradient,
  4 feedback, 5 steady, 6 off).

Static purple:

```toml
loop_end = 0
feedback = 0

[leds]
bottom = ["#8000ff", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000"]
# same for middle_right, top_right, top_left, middle_left
```

## Rumble (`export` / `import`)

- `rumble.enabled`: the official app's "Controller Vibration" switch. When false the controller ignores all
  rumble from the host. `left` is the low-frequency, `right` the high-frequency grip motor; `scale` is the
  intensity in %, `min`/`max` (0-255) are only exposed in the app's debug mode.
- `trigger_rumble.enabled`: the app's "Trigger Vibration" switch. In `linear`, `scale` is the intensity in %,
  `filter` the "masked value" (weaker rumble is ignored), `min_level`/`max_level` the amplitude range.
  Plain XInput rumble did not make the triggers vibrate in tests.
- `trigger_adapter` (adaptive triggers) is only used by the Apex series and kept as raw bytes.

## Controller quirks this works around

- The controller never sends a reply identical to its previous reply, so repeated commands need a
  different command in between.
- Reading or writing a slot makes it the active slot; the previously active slot is restored.
- Writes change a RAM working copy; `50 03 <version>` saves the whole working copy (mapping and LED)
  into the active slot, so the slot is loaded with a real switch before writing.
- After switches and saves the controller sometimes ignores commands for several seconds, and some
  seconds after a save it occasionally drops its radio link, making the dongle re-enumerate on USB.
  flugdiggi retries and reopens the device in that case.

Protocol notes and logs live in the separate `workdir` repository.
