//! Prints the string descriptors (or the errors reading them) of all 045e:028e devices.
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    for dev in rusb::devices()?.iter() {
        let desc = dev.device_descriptor()?;
        if desc.vendor_id() != 0x045e || desc.product_id() != 0x028e {
            continue;
        }
        println!("bus {} addr {}", dev.bus_number(), dev.address());
        let h = dev.open()?;
        println!("  languages: {:?}", h.read_languages(Duration::from_secs(1)));
        println!("  manufacturer: {:?}", h.read_manufacturer_string_ascii(&desc));
        println!("  product: {:?}", h.read_product_string_ascii(&desc));
    }
    Ok(())
}
