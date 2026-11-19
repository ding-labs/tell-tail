//! Synthetic metadata only; exercises the real tool store without a receiver or user files.
use crate::tool_trace::{
    live::Shared,
    model::{Observation, Phase},
};

pub fn seed(traces: &Shared) {
    let now = crate::mcp::time::now_ms();
    let mut state = traces.lock().unwrap();
    state.enabled = true;
    state.replay = true;
    state.last_received_ms = Some(now);
    for (i, (plugin, tool, phase, success)) in [
        ("demo-docs", "search", Phase::Completed, Some(true)),
        ("demo-deploy", "status", Phase::Failed, Some(false)),
        ("demo-files", "list", Phase::Completed, None),
        ("demo-calendar", "create", Phase::Denied, None),
    ]
    .into_iter()
    .enumerate()
    {
        state
            .store
            .accept(
                Observation {
                    schema_version: 1,
                    source: "synthetic".into(),
                    source_epoch: "demo".into(),
                    execution_id: "demo-execution".into(),
                    call_id: Some(format!("demo-{i}")),
                    event_id: None,
                    tool: tool.into(),
                    server: Some("demo-tools".into()),
                    plugin: Some(plugin.into()),
                    phase,
                    timestamp_ms: Some(now - (4 - i as i64) * 1000),
                    duration_ms: (phase != Phase::Denied).then_some(123 + i as u64 * 71),
                    execution_success: success,
                    tool_error: None,
                },
                now,
                true,
            )
            .expect("valid synthetic observation");
    }
}
