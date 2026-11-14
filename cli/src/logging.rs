use std::fmt;
use std::io::IsTerminal;

use anstyle::{AnsiColor, Style};
use tracing::{Event, Level, Subscriber};
use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Logs go to stderr: info and above by default, warnings and errors with `quiet`, everything but
/// trace with `verbose`. `RUST_LOG` adds directives on top.
pub fn init(verbose: bool, quiet: bool) -> color_eyre::Result<()> {
    color_eyre::install()?;
    let level = match (verbose, quiet) {
        (true, _) => Level::DEBUG,
        (false, true) => Level::WARN,
        (false, false) => Level::INFO,
    };
    let filter = EnvFilter::builder().with_default_directive(level.into()).from_env_lossy();
    let ansi = std::io::stderr().is_terminal();
    let output = if verbose {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(ansi)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .boxed()
    } else {
        tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_ansi(ansi).event_format(Plain).boxed()
    };
    tracing_subscriber::registry().with(output.with_filter(filter)).with(ErrorLayer::default()).try_init()?;
    Ok(())
}

/// `warning: message` without timestamps or spans; info lines have no prefix.
struct Plain;

impl<S, N> FormatEvent<S, N> for Plain
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, ctx: &FmtContext<'_, S, N>, mut w: Writer<'_>, event: &Event<'_>) -> fmt::Result {
        let prefix = match *event.metadata().level() {
            Level::ERROR => Some(("error", AnsiColor::Red)),
            Level::WARN => Some(("warning", AnsiColor::Yellow)),
            Level::INFO => None,
            _ => Some(("debug", AnsiColor::Blue)),
        };
        if let Some((label, color)) = prefix {
            let style =
                if w.has_ansi_escapes() { Style::new().fg_color(Some(color.into())).bold() } else { Style::new() };
            write!(w, "{style}{label}:{style:#} ")?;
        }
        ctx.field_format().format_fields(w.by_ref(), event)?;
        writeln!(w)
    }
}
