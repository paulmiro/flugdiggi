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
- The LED shown is the active slot's LED config. Unverified: after power-on the controller may show the
  LED config of the active Switch-mode slot instead until the first slot switch.

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
