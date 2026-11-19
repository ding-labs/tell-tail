//! Domain projections preserve qualified outcomes and opaque domain-local scope.
use super::model::{Category, Evidence, Item, Severity};
use crate::{
    mcp::model::{Event, Host, Kind},
    tool_trace::model::{Call, DurationBasis, Status},
};

pub fn mcp(event: &Event) -> Item {
    let mut item = Item::observation(Category::Mcp, "mcp_observation", "MCP observation");
    item.summary = event.summary.clone();
    item.subject = event.server.clone();
    item.server = Some(event.server.clone());
    item.client = match event.host {
        Host::CodexDesktop => "chatgpt_codex_desktop",
        Host::ClaudeDesktop => "claude_desktop",
    };
    item.source = "mcp_logs";
    item.adapter = "mcp-log-v1";
    item.source_level = event.level;
    item.source_ref = event
        .evidence
        .first()
        .map(|e| format!("file-{}", e.source_id))
        .unwrap_or_default();
    item.scope = if event.scope_known {
        "server_execution"
    } else {
        "unknown"
    };
    item.scope_ref = event.scope_known.then(|| format!("mcp-{}", event.scope));
    item.occurred_ms = event.timestamp_ms;
    item.received_ms = event.observed_ms;
    item.historical = event.historical;
    item.method = event.method;
    item.diagnostic = matches!(event.kind, Kind::Protocol | Kind::Unknown);
    item.outcome = match event.kind {
        Kind::Ready => "ready_observed",
        Kind::Starting => "start_observed",
        Kind::Cancelled => "cancelled",
        kind if kind.is_failure() => "failure_reported",
        Kind::Closed | Kind::IntentionalClose | Kind::RendererRelease => "closed_observed",
        _ => "observed",
    };
    item.kind = match event.kind {
        Kind::Ready => "mcp_ready",
        Kind::Starting => "mcp_starting",
        Kind::DiscoveryFailed => "mcp_discovery_failed",
        Kind::DiagnosticFailed => "mcp_diagnostic_failed",
        Kind::UnexpectedClose => "mcp_unexpected_close",
        Kind::Failed => "mcp_startup_failed",
        _ => "mcp_observation",
    };
    item.severity = if event.kind.is_failure() {
        Severity::Error
    } else {
        Severity::Info
    };
    item.advice = event.reason.map(|r| r.advice());
    item.note = "Server state is a last observation, not a health check. Server executions are distinct from chats.";
    item.evidence = event
        .evidence
        .iter()
        .take(4)
        .map(|e| Evidence {
            source: format!("file-{}", e.source_id),
            generation: e.generation,
            offset: e.offset,
        })
        .collect();
    item
}

pub fn call(call: &Call) -> Item {
    let mut item = Item::observation(Category::Tools, "tool_call", "Tool call");
    item.subject = call.tool.clone();
    item.summary = call.tool.clone();
    item.client = "codex_exporter";
    item.source = "tool_telemetry";
    item.adapter = "codex-otlp-tool-result-v1";
    item.plugin = call.plugin.clone();
    item.server = call.server.clone();
    item.source_ref = format!("trace-{}", call.source);
    item.scope = "unlinked_execution";
    item.scope_ref = Some(format!("tool-{}", call.execution));
    item.occurred_ms = call.started_ms.or(call.completed_ms);
    item.received_ms = call.received_ms;
    item.historical = call.historical;
    item.duration_ms = call.duration_ms;
    item.duration_basis = call.duration_basis.map(|b| match b {
        DurationBasis::SourceReported => "source_reported",
        DurationBasis::SourceTimestamps => "source_timestamps",
    });
    item.start_observed = call.start_observed;
    item.completion_observed = call.completion_observed;
    item.execution_success = call.execution_success;
    item.tool_error = call.tool_error;
    item.gap_observed = call.gap_observed;
    item.outcome = match call.status {
        Status::Running => "start_observed",
        Status::Failed => "execution_failed",
        Status::Cancelled => "cancelled",
        Status::Denied => "denied",
        Status::Incomplete => "incomplete",
        Status::Conflict => "conflicting_evidence",
        Status::Completed if call.tool_error == Some(true) => "tool_error",
        Status::Completed if call.execution_success == Some(true) => "execution_succeeded",
        Status::Completed => "completed",
    };
    item.severity = match item.outcome {
        "execution_failed" | "tool_error" => Severity::Error,
        "conflicting_evidence" => Severity::Warning,
        _ => Severity::Info,
    };
    item.note = "Execution outcome and tool-reported error are separate. Missing starts, parent links and chat identity are not inferred.";
    item
}

#[cfg(test)]
mod tests {
    use crate::tool_trace::{model::Observation, store::Store};
    #[test]
    fn call_projection_preserves_unknown_outcomes_and_does_not_guess_chat_links() {
        let observation: Observation = serde_json::from_value(serde_json::json!({"schema_version":1,"source":"fixture","source_epoch":"a","execution_id":"PRIVATE","call_id":"call","tool":"example","phase":"completed","timestamp_ms":100})).unwrap();
        let call = Store::default()
            .accept(observation, 101, false)
            .unwrap()
            .unwrap();
        let item = super::call(&call);
        assert_eq!(item.outcome, "completed");
        assert!(!item.start_observed);
        assert_eq!(item.scope, "unlinked_execution");
        assert!(!serde_json::to_string(&item).unwrap().contains("PRIVATE"));
        let mut failed = call;
        failed.execution_success = Some(true);
        failed.tool_error = Some(true);
        assert_eq!(super::call(&failed).outcome, "tool_error");
    }
}
