use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand};
use flugdiggi_core::{Selector, Slot};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Cyan.on_default().bold())
    .invalid(AnsiColor::Yellow.on_default().bold())
    .error(AnsiColor::Red.on_default().bold());

const AFTER_HELP: &str = "\
--device is required when several controllers are connected. Selecting by USB port (e.g. 3-3) touches no \
other device; selecting by MAC briefly disconnects the other controllers from games.

Slots 0-3 are the XInput-mode configs, 4-7 the Switch-mode configs. Every write first saves the slot's \
previous content to flugdiggi-backup-<kind>-slot<N>-<time>.bin in the current directory.

Typical edit: flugdiggi export 1 cfg.toml, edit cfg.toml, flugdiggi import 1 cfg.toml";

/// Configure Flydigi Vader 4 controllers (button mapping, sticks, triggers, macros, LEDs)
#[derive(Debug, Parser)]
#[command(name = "flugdiggi", version, styles = STYLES, after_help = AFTER_HELP)]
pub struct Cli {
    /// Show debug output, including every packet
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Only show warnings and errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Controller to use: USB port (e.g. 3-3) or MAC (e.g. 38:33:FD:F5), see `flugdiggi list`
    #[arg(short, long, global = true, value_name = "PORT|MAC")]
    pub device: Option<Selector>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List connected Flydigi controllers by USB port and connection
    ///
    /// Without --mac no controller is claimed, and the connection type is a guess (shown as "cable?" or
    /// "dongle?"): a wired Vader 4 Pro (firmware 6.9.5.5) refuses the USB manufacturer string request.
    List {
        /// Also show MACs, firmware and the exact connection. Briefly disconnects every controller from games.
        #[arg(long)]
        mac: bool,
    },
    /// Show device info, the active slot and the slot versions
    Info,
    /// Show the active slot, or switch to another one
    Slot {
        /// Slot to activate
        #[arg(value_parser = clap::value_parser!(u8).range(0..=3))]
        slot: Option<u8>,
    },
    /// Print or save a slot's config as TOML
    Export {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: Option<PathBuf>,
    },
    /// Write a (modified) TOML config to a slot
    Import {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Rename a slot's config
    Rename {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        /// New name, at most 8 characters
        name: String,
    },
    /// Save a slot's raw config blob
    Dump {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Write a raw config blob back to a slot
    Restore {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Print a raw config or LED blob as TOML (offline)
    Decode { file: PathBuf },
    /// Print or save a slot's LED config as TOML
    LedExport {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: Option<PathBuf>,
    },
    /// Write a (modified) TOML LED config to a slot
    LedImport {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Save a slot's raw LED blob
    LedDump {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Write a raw LED blob back to a slot
    LedRestore {
        #[arg(value_parser = parse_slot)]
        slot: Slot,
        file: PathBuf,
    },
    /// Send read-only commands and print all replies
    Raw {
        /// Command bytes in hex, separated by colons (allowed: 10[:xx], 20, 50:04)
        #[arg(required = true, value_name = "CMD[:ARG...]", value_parser = parse_hex)]
        commands: Vec<Vec<u8>>,
    },
}

fn parse_slot(s: &str) -> Result<Slot, String> {
    s.parse().map_err(|_| "must be a slot number 0-7".to_owned())
}

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    s.split(':').map(|b| u8::from_str_radix(b, 16).map_err(|_| format!("{b:?} is not a hex byte"))).collect()
}
