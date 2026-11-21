use super::model::{Call, DurationBasis, Observation, Phase, Status};

#[derive(Clone, PartialEq, Eq)]
struct Terminal {
    phase: Phase,
    time: Option<i64>,
    duration: Option<u64>,
    execution_success: Option<bool>,
    tool_error: Option<bool>,
}

pub(super) struct Tracked {
    pub call: Call,
    terminal: Option<Terminal>,
}

impl Tracked {
    pub fn new(
        id: u64,
        source: String,
        execution: String,
        observation: &Observation,
        received_ms: i64,
        historical: bool,
    ) -> Self {
        Self {
            call: Call {
                inspection: observation.inspection.clone(),
                id,
                revision: 0,
                source,
                execution,
                correlated: observation.call_id.is_some(),
                tool: observation.tool.clone(),
                server: observation.server.clone(),
                plugin: observation.plugin.clone(),
                status: Status::Running,
                started_ms: None,
                completed_ms: None,
                received_ms,
                duration_ms: None,
                duration_basis: None,
                execution_success: None,
                tool_error: None,
                start_observed: false,
                completion_observed: false,
                gap_observed: false,
                historical,
            },
            terminal: None,
        }
    }

    pub fn observe(&mut self, o: &Observation) -> bool {
        let before = self.call.clone();
        let c = &mut self.call;
        if c.tool != o.tool || c.server != o.server || c.plugin != o.plugin {
            c.status = Status::Conflict;
            return *c != before;
        }
        if o.phase == Phase::Started {
            if c.start_observed && c.started_ms != o.timestamp_ms {
                c.status = Status::Conflict;
            } else {
                c.start_observed = true;
                c.started_ms = o.timestamp_ms;
            }
        } else {
            let t = Terminal {
                phase: o.phase,
                time: o.timestamp_ms,
                duration: o.duration_ms,
                execution_success: o.execution_success,
                tool_error: o.tool_error,
            };
            if self.terminal.as_ref().is_some_and(|old| old != &t) {
                // Preserve the original evidence and mark disagreement explicitly.
                c.status = Status::Conflict;
            } else {
                self.terminal = Some(t);
                c.completion_observed = true;
                c.completed_ms = o.timestamp_ms;
                c.execution_success = o.execution_success;
                c.tool_error = o.tool_error;
                if c.status != Status::Conflict {
                    c.status = match o.phase {
                        Phase::Completed
                            if o.tool_error == Some(true) || o.execution_success == Some(false) =>
                        {
                            Status::Failed
                        }
                        Phase::Completed => Status::Completed,
                        Phase::Failed => Status::Failed,
                        Phase::Cancelled => Status::Cancelled,
                        Phase::Denied => Status::Denied,
                        Phase::Started => unreachable!(),
                    };
                }
                if let Some(duration) = o.duration_ms {
                    c.duration_ms = Some(duration);
                    c.duration_basis = Some(DurationBasis::SourceReported);
                }
            }
        }
        if c.duration_basis != Some(DurationBasis::SourceReported) && c.status != Status::Denied {
            c.duration_ms = c
                .completed_ms
                .zip(c.started_ms)
                .and_then(|(end, start)| end.checked_sub(start))
                .and_then(|n| u64::try_from(n).ok())
                .filter(|&n| n <= 86_400_000);
            c.duration_basis = c.duration_ms.map(|_| DurationBasis::SourceTimestamps);
        }
        *c != before
    }

    pub fn gap(&mut self) -> bool {
        if self.call.completion_observed || self.call.gap_observed {
            return false;
        }
        self.call.gap_observed = true;
        if self.call.status != Status::Conflict {
            self.call.status = Status::Incomplete;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(phase: Phase, time: Option<i64>) -> Observation {
        Observation {
            inspection: None,
            schema_version: 1,
            source: "private-source".into(),
            source_epoch: "private-epoch".into(),
            execution_id: "private-execution".into(),
            call_id: Some("private-call".into()),
            event_id: None,
            tool: "docs.search".into(),
            server: None,
            plugin: Some("example".into()),
            phase,
            timestamp_ms: time,
            duration_ms: None,
            execution_success: None,
            tool_error: None,
        }
    }
    fn tracked(o: &Observation) -> Tracked {
        Tracked::new(
            1,
            "local-source".into(),
            "local-execution".into(),
            o,
            500,
            false,
        )
    }
    #[test]
    fn late_start_enriches_result_without_reverting_completion() {
        let o = observation(Phase::Completed, Some(100));
        let mut t = tracked(&o);
        assert!(t.observe(&o));
        assert!(!t.observe(&o));
        assert_eq!(t.call.duration_ms, None);
        assert!(t.observe(&observation(Phase::Started, Some(80))));
        assert_eq!(t.call.status, Status::Completed);
        assert_eq!(t.call.duration_ms, Some(20));
        assert_eq!(t.call.execution_success, None);
    }
    #[test]
    fn source_duration_wins_and_clock_reversal_is_not_latency() {
        let mut o = observation(Phase::Completed, Some(50));
        let mut t = tracked(&o);
        t.observe(&observation(Phase::Started, Some(100)));
        t.observe(&o);
        assert_eq!(t.call.duration_ms, None);
        o.duration_ms = Some(12);
        let mut t = tracked(&o);
        t.observe(&o);
        t.observe(&observation(Phase::Started, Some(100)));
        assert_eq!(t.call.duration_ms, Some(12));
        assert_eq!(t.call.duration_basis, Some(DurationBasis::SourceReported));
    }
    #[test]
    fn gap_does_not_invent_timeout_and_late_completion_preserves_gap() {
        let o = observation(Phase::Started, Some(1));
        let mut t = tracked(&o);
        t.observe(&o);
        assert!(t.gap());
        assert_eq!(t.call.status, Status::Incomplete);
        t.observe(&observation(Phase::Completed, Some(5)));
        assert_eq!(t.call.status, Status::Completed);
        assert!(t.call.gap_observed);
    }
    #[test]
    fn tool_failure_and_conflicting_identity_do_not_become_success() {
        let mut o = observation(Phase::Completed, Some(1));
        o.execution_success = Some(true);
        o.tool_error = Some(true);
        let mut t = tracked(&o);
        t.observe(&o);
        assert_eq!(t.call.status, Status::Failed);
        o.tool = "different.tool".into();
        t.observe(&o);
        assert_eq!(t.call.status, Status::Conflict);
        assert_eq!(t.call.tool, "docs.search");
        assert!(!serde_json::to_string(&t.call).unwrap().contains("private"));
    }
    #[test]
    fn terminal_disagreement_is_sticky() {
        let o = observation(Phase::Completed, Some(1));
        let mut t = tracked(&o);
        t.observe(&o);
        t.observe(&observation(Phase::Failed, Some(1)));
        t.observe(&o);
        assert_eq!(t.call.status, Status::Conflict);
    }
}
