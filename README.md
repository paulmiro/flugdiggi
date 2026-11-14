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

- Slots 0-3 are the XInput-mode configs, 4-7 the Switch-mode configs; all are readable and writable.
  Writing a Switch-mode slot briefly makes it the active slot; afterwards both the active slot and the
  active Switch-mode slot are restored.
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

## Buttons (`export` / `import`)

```toml
[buttons]          # only buttons that do something other than themselves
c = "ls"
m1 = "b"

[turbo.a]          # only buttons with turbo
rate = 10          # presses per second (1-30 in the official app)
mode = "hold"      # "hold" (while pressed), "toggle" (on/off by click) or "off"
```

- A button left out of `[buttons]` acts as itself; extra buttons without an Xbox equivalent (`c`, `z`,
  `m1`-`m4`, `fn`) then send nothing. `a = "a"` is the same as leaving `a` out.
- Vader 4 Pro buttons: `up right down left a b x y select start lb rb lt rt ls rs c z m1 m2 m3 m4 fn home`
  (`fn` = FN button, `home` = power button). The table also has `m5 m6 turbo back key26 key29`-`key31`
  for other models. Names are case-insensitive; `l1`/`r1`, `l2`/`r2`, `l3`/`r3` and `menu` (= `fn`) are aliases.
- Targets: `up right down left a b x y select start lb rb lt rt ls rs` (or their aliases), `home` (Xbox
  guide button; not offered by the official app, but works), `none` (button sends nothing), or `macro`
  (set automatically for buttons that have a macro). Anything else is rejected.
- Entries you don't change keep their exact bytes.

## Macros (`export` / `import`)

A macro replaces its button with a timed sequence of button events (up to 5 macros per config):

```toml
[macros]
intervals_ms = [30, 30, 30, 30, 30]   # pause between repetitions, per macro (modes 2 and 3)

[[macros.list]]
button = "a"       # pressing A plays this instead of A
mode = 1           # 1 = once per press, 2 = repeat while held, 3 = toggle repeat by click

[[macros.list.steps]]
at_ms = 0          # time since the press, steps of 10 ms
key = "b"
event = 1          # 1 = press, 0 = release; 2/3 = move the left/right stick (see below)

[[macros.list.steps]]
at_ms = 100
key = "b"
event = 0
```

- The macro's button gets the `"macro"` target in `[buttons]` automatically (the controller does the same)
  and no longer sends its own input.
- Mode 2 repeats the macro while the button is held, mode 3 starts repeating on one click and stops on the
  next; each run is followed by the pause from `intervals_ms`.
- Stick steps: `event = 2` (left) or `3` (right) with `key` = `stick_up`, `stick_up_right`, `stick_right`,
  `stick_down_right`, `stick_down`, `stick_down_left`, `stick_left`, `stick_up_left` or `stick_center`.
  The stick is fully deflected until the next step for that stick; end with `stick_center`.
- All modes, intervals and stick steps were tested on hardware; timing was exact to about 1 ms.

## Rumble (`export` / `import`)

- `rumble.enabled`: the official app's "Controller Vibration" switch. When false the controller ignores all
  rumble from the host. `left` is the low-frequency, `right` the high-frequency grip motor; `scale` is the
  intensity in %, `min`/`max` (0-255) are only exposed in the app's debug mode.
- `trigger_rumble.enabled`: the app's "Trigger Vibration" switch. In `linear`, `scale` is the intensity in %,
  `filter` the "masked value" (weaker rumble is ignored), `min_level`/`max_level` the amplitude range.
  The Vader 4 Pro has no trigger motors (teardown), so this block has no effect on it.
- `trigger_adapter` (adaptive triggers) is only used by the Apex series and kept as raw bytes.

## Controller quirks this works around

- The controller never sends a reply identical to its previous reply, so repeated commands need a
  different command in between.
- Reading a slot 0-3 or writing any slot makes it the active slot; the previously active slot is restored.
- Switching to a Switch-mode slot (4-7) makes it both the active slot and the active Switch-mode slot
  (reading one does not). The official app writes these slots with a "write all" transfer instead;
  flugdiggi uses the same partial write + save as for slots 0-3.
- Writes change a RAM working copy; `50 03 <version>` saves the whole working copy (mapping and LED)
  into the active slot, so the slot is loaded with a real switch before writing.
- After switches and saves the controller sometimes ignores commands for several seconds, and some
  seconds after a save it occasionally drops its radio link, making the dongle re-enumerate on USB.
  flugdiggi waits up to 60 s for it and reopens the device after a re-enumeration.
- Sometimes the controller answers LED reads with mapping data; a full mapping read (or a slot switch)
  clears that. If it ignores the start of a write, the slot is reloaded and the write restarted, since
  pushing such a write through often saved an outdated LED config.

Protocol notes and logs live in the separate `workdir` repository.
