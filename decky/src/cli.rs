use std::path::PathBuf;

use clap::{Parser, Subcommand};
use flugdiggi_core::{Slot, UsbPort};

/// Helper of the flugdiggi Decky plugin. Prints one JSON object on stdout.
#[derive(Debug, Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Button and target tables the frontend builds its lists from.
    Meta,
    /// Connected controllers (claims none of them).
    List,
    /// Device info and active slot.
    Status {
        #[arg(long)]
        port: UsbPort,
    },
    /// Button mapping of a slot.
    ReadButtons {
        #[arg(long)]
        port: UsbPort,
        #[arg(long)]
        slot: Slot,
    },
    /// Replaces the button mapping of a slot. Reads `{"buttons": {...}, "expected_data_version": n}` from stdin.
    WriteButtons {
        #[arg(long)]
        port: UsbPort,
        #[arg(long)]
        slot: Slot,
        /// Where the previous content of the slot is saved.
        #[arg(long)]
        backup_dir: PathBuf,
    },
    /// Makes a slot (0-3) the active one.
    Activate {
        #[arg(long)]
        port: UsbPort,
        #[arg(long)]
        slot: Slot,
    },
}
