//! Button and target tables, as in the GUI (`gui/src/model.rs`).

/// The remappable buttons of the Vader 4 Pro, in display order.
pub const BUTTONS: [&str; 22] = [
    "a", "b", "x", "y", "lb", "rb", "lt", "rt", "ls", "rs", "select", "start", "up", "down", "left", "right", "c", "z",
    "m1", "m2", "m3", "m4",
];

/// Buttons without an Xbox 360 equivalent; by default they send nothing.
pub fn is_extra(button: &str) -> bool {
    matches!(button, "c" | "z" | "m1" | "m2" | "m3" | "m4")
}

/// Targets a button can be mapped to.
pub const TARGETS: [&str; 17] =
    ["a", "b", "x", "y", "lb", "rb", "lt", "rt", "ls", "rs", "select", "start", "up", "down", "left", "right", "home"];

pub fn label(key: &str) -> String {
    match key {
        "select" => "Select".into(),
        "start" => "Start".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "home" => "Guide".into(),
        "none" => "Nothing".into(),
        "macro" => "Macro".into(),
        k => k.to_uppercase(),
    }
}
