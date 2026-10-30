//! Send structured manager events to the owner-only deployment log stream.

use crate::log_store::LogStore;
use anyhow::Result;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
};

struct EventFields {
    code: String,
    message: String,
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "event_code" => self.code = format!("{value:?}"),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_owned(),
            "event_code" => self.code = value.to_owned(),
            _ => {}
        }
    }
}

struct ManagerLayer(LogStore);

impl<S: Subscriber> Layer<S> for ManagerLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut fields = EventFields {
            code: "manager_event".into(),
            message: String::new(),
        };
        event.record(&mut fields);
        let level = event.metadata().level().as_str().to_ascii_lowercase();
        let _ = self
            .0
            .append_event("isos", &level, &fields.code, &fields.message);
    }
}

pub fn install(store: LogStore) -> Result<()> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(ManagerLayer(store)),
    )?;
    Ok(())
}
