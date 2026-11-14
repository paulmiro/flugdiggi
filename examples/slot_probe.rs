//! Switches to each given slot (50 05 <slot>) and prints the reported active slots.
//! Usage: slot_probe <slot>...

use anyhow::Result;
use flugdiggi::old::Controller;

fn main() -> Result<()> {
    let c = Controller::open()?;
    println!("start: {:?}", c.current_config_id()?);
    for s in std::env::args().skip(1) {
        let r = c.apply_config(s.parse()?);
        println!("switch {s}: {r:?} -> {:?}", c.current_config_id()?);
    }
    Ok(())
}
