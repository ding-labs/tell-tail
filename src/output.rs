use crate::{cli::Output, parse::Level, stats::Snapshot};
use std::io;

pub fn render(snapshot: &Snapshot, output: Output) -> io::Result<String> {
    if output == Output::Prometheus {
        return Ok(crate::prometheus::render(snapshot));
    }
    if output == Output::Json {
        return serde_json::to_string(snapshot)
            .map(|s| s + "\n")
            .map_err(io::Error::other);
    }
    if !matches!(output, Output::Text | Output::Auto) {
        return Err(io::Error::other("this output is not implemented yet"));
    }
    let levels = Level::ALL
        .iter()
        .map(|&l| format!("{}={}", l.name(), snapshot.totals.levels.get(l)))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!(
        "{} source={} state={:?} records={} bytes={} elapsed={:.3}s window={:.2}s entries/s={:.1} MiB/s={:.2} warn={} error={} {} parse_failures={} oversized={} pending_bytes={}\n",
        snapshot.kind,
        snapshot.source.escape_debug(),
        snapshot.state,
        snapshot.totals.records,
        snapshot.bytes_read,
        snapshot.elapsed_seconds,
        snapshot.window.observed_seconds,
        snapshot.window.records_per_second,
        snapshot.window.bytes_per_second / 1_048_576.0,
        percent(snapshot.window.warning_ratio),
        percent(snapshot.window.error_ratio),
        levels,
        snapshot.totals.parse_failures,
        snapshot.totals.oversized_records,
        snapshot.pending_bytes
    ))
}
pub fn percent(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.2}%", v * 100.0))
        .unwrap_or_else(|| "—".into())
}
