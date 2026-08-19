use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

const TUI_LOG_CAPACITY: usize = 256;

pub fn init_cli() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize tracing: {err}"))?;

    Ok(())
}

pub fn init_tui() -> anyhow::Result<Receiver<String>> {
    let (tx, rx) = mpsc::sync_channel(TUI_LOG_CAPACITY);

    tracing_subscriber::registry()
        .with(env_filter())
        .with(TuiLogLayer::new(tx))
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize tracing: {err}"))?;

    Ok(rx)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

struct TuiLogLayer {
    tx: SyncSender<String>,
}

impl TuiLogLayer {
    fn new(tx: SyncSender<String>) -> Self {
        Self { tx }
    }
}

impl<S> Layer<S> for TuiLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let message = visitor.finish();
        let line = format!(
            "{} {}: {}",
            event.metadata().level(),
            event.metadata().target(),
            message
        );

        let _ = self.tx.try_send(line);
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl EventVisitor {
    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn finish(self) -> String {
        let mut values = Vec::with_capacity(1 + self.fields.len());
        if let Some(message) = self.message {
            values.push(message);
        }
        values.extend(self.fields);
        values.join(" ")
    }
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_events_to_tui() {
        let (tx, rx) = mpsc::sync_channel(1);
        let subscriber = tracing_subscriber::registry().with(TuiLogLayer::new(tx));

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                target: "sophon_lib::installer",
                "107342 Chunks to download, 2673 Files to install"
            );
        });

        assert_eq!(
            rx.try_recv().unwrap(),
            "INFO sophon_lib::installer: 107342 Chunks to download, 2673 Files to install"
        );
    }
}
