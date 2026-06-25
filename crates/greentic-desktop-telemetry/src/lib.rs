use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryEvent {
    pub name: String,
    pub detail: String,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryLog {
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
}

impl TelemetryLog {
    pub fn record(&self, name: impl Into<String>, detail: impl Into<String>) {
        let mut events = self.events.lock().expect("telemetry mutex poisoned");
        events.push(TelemetryEvent {
            name: name.into(),
            detail: detail.into(),
            timestamp: SystemTime::now(),
        });
    }

    pub fn events(&self) -> Vec<TelemetryEvent> {
        self.events
            .lock()
            .expect("telemetry mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_events() {
        let log = TelemetryLog::default();
        log.record("tool_call", "desktop.info");

        let events = log.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "tool_call");
        assert_eq!(events[0].detail, "desktop.info");
    }
}
