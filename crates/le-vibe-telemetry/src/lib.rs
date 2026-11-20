use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpanRecord {
    pub name: String,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Debug, Default, Clone)]
pub struct TelemetryRecorder {
    spans: Arc<Mutex<Vec<SpanRecord>>>,
}

impl TelemetryRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_span(&self, name: impl Into<String>) -> usize {
        let now = now_ms();
        let mut guard = self.spans.lock().expect("telemetry lock");
        let idx = guard.len();
        guard.push(SpanRecord {
            name: name.into(),
            started_at_ms: now,
            ended_at_ms: None,
        });
        idx
    }

    pub fn finish_span(&self, idx: usize) {
        let now = now_ms();
        let mut guard = self.spans.lock().expect("telemetry lock");
        if let Some(span) = guard.get_mut(idx) {
            span.ended_at_ms = Some(now);
        }
    }

    pub fn spans(&self) -> Vec<SpanRecord> {
        self.spans.lock().expect("telemetry lock").clone()
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_finishes_spans() {
        let rec = TelemetryRecorder::new();
        let id = rec.record_span("agent.turn");
        rec.finish_span(id);
        let spans = rec.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "agent.turn");
        assert!(spans[0].ended_at_ms.is_some());
    }

    #[test]
    fn tracks_overlapping_spans_and_ignores_out_of_bounds_finish() {
        let rec = TelemetryRecorder::new();
        let outer = rec.record_span("turn");
        let inner = rec.record_span("tool.read");
        rec.finish_span(inner);
        rec.finish_span(999);
        let spans = rec.spans();
        assert_eq!(spans.len(), 2);
        assert!(spans[outer].ended_at_ms.is_none());
        assert!(spans[inner].ended_at_ms.is_some());
    }
}
