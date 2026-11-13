use anyhow::{Result, bail};
use flugdiggi::old::Controller;
use flugdiggi::transport::hex;

fn usage() -> ! {
    eprintln!("usage: flugdiggi [-v] <info | slot [N] | dump-config <slot> <file>>");
    std::process::exit(2)
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let trace = args.first().is_some_and(|a| a == "-v");
    if trace {
        args.remove(0);
    }
    let Some(sub) = args.first() else { usage() };

    let mut c = Controller::open()?;
    c.t.trace = trace;

    match sub.as_str() {
        "info" => {
            println!("product: {}", c.t.product);
            println!("{:#?}", c.heartbeat()?);
            let (cur, ns) = c.current_config_id()?;
            println!("active slot: {cur} (switch mode slot: {ns:?})");
            println!("slot versions: {:04x?}", c.config_versions()?);
        }
        "slot" => match args.get(1) {
            None => println!("{}", c.current_config_id()?.0),
            Some(n) => {
                let n: u8 = n.parse()?;
                if n > 3 {
                    bail!("slot must be 0..=3");
                }
                c.apply_config(n)?;
                println!("active slot now {}", c.current_config_id()?.0);
            }
        },
        "dump-config" => {
            let (Some(slot), Some(file)) = (args.get(1), args.get(2)) else { usage() };
            let data = c.read_mapping_config_raw(slot.parse()?)?;
            std::fs::write(file, &data)?;
            println!("{} bytes written to {file}", data.len());
            if trace {
                for chunk in data.chunks(16) {
                    println!("{}", hex(chunk));
                }
            }
        }
        _ => usage(),
    }
    Ok(())
}
