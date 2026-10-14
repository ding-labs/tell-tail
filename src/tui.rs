use crate::{
    output::percent, parse::Level, platform::StdoutWriter, runtime::Running, stats::Snapshot,
};
use crossterm::{
    cursor::{MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table},
};
use std::{
    cell::RefCell,
    io::{self, IsTerminal, Write},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub fn available() -> bool {
    io::stdout().is_terminal()
        && std::env::var("TERM").is_ok_and(|term| term != "dumb")
        && std::fs::File::open("/dev/tty").is_ok()
}
struct SharedWriter(Rc<RefCell<StdoutWriter>>);
impl Write for SharedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}
struct Restore {
    terminal: Option<Terminal<CrosstermBackend<SharedWriter>>>,
    writer: Rc<RefCell<StdoutWriter>>,
}
fn restore_screen() {
    let _ = terminal::disable_raw_mode();
    if let Ok(mut writer) = StdoutWriter::new(Arc::new(AtomicUsize::new(1))) {
        let _ = execute!(writer, LeaveAlternateScreen, Show);
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        // Stop an in-flight frame before sending cleanup through a fresh writer.
        self.writer.borrow_mut().discard();
        drop(self.terminal.take());
        restore_screen();
    }
}
pub fn run(
    running: &mut Running,
    writer: StdoutWriter,
    stop: Arc<AtomicUsize>,
) -> io::Result<Snapshot> {
    std::panic::set_hook(Box::new(|info| {
        let _ = terminal::disable_raw_mode();
        crate::writer::diagnostic(&format!("tell-tail: {info}\n"));
        // The guard restores the screen during unwinding, after stopping output.
    }));
    let writer = Rc::new(RefCell::new(writer));
    let terminal = Terminal::new(CrosstermBackend::new(SharedWriter(writer.clone())))?;
    let mut restore = Restore {
        terminal: Some(terminal),
        writer,
    };
    terminal::enable_raw_mode()?;
    let terminal = restore.terminal.as_mut().expect("active terminal");
    // Fullscreen output does not need a cursor-position query (which can wait
    // several seconds for a terminal response and bypass our output writer).
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )?;
    let mut frozen: Option<Snapshot> = None;
    let mut help = false;
    loop {
        let current = running.snapshot();
        terminal.draw(|frame| {
            draw(
                frame,
                frozen.as_ref().unwrap_or(&current),
                frozen.is_some(),
                help,
            )
        })?;
        if current.state.is_final() {
            return Ok(current);
        }
        if running.is_finished() {
            running.join()?;
            continue;
        }
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => running.stop(),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        stop.store(signal_hook::consts::SIGINT as usize, Ordering::Relaxed)
                    }
                    KeyCode::Char(' ') => {
                        frozen = if frozen.is_some() {
                            None
                        } else {
                            Some(current)
                        }
                    }
                    KeyCode::Char('?') => help = !help,
                    KeyCode::Esc => help = false,
                    _ => (),
                }
            }
        }
    }
}
pub fn draw(frame: &mut Frame, s: &Snapshot, frozen: bool, help: bool) {
    let area = frame.area();
    let fg = Color::Rgb(215, 225, 235);
    let accent = Color::Rgb(93, 216, 203);
    frame.render_widget(Block::default().style(Style::default().fg(fg)), area);
    if area.width < 60 || area.height < 22 {
        frame.render_widget(Paragraph::new(format!("Tell-Tail · {:?}\n{} records · {} bytes\nwarn {} · error {}\nResize to 60 × 22 for dashboard\nq quit · space freeze", s.state, s.totals.records, s.bytes_read, percent(s.window.warning_ratio), percent(s.window.error_ratio))), area);
        return;
    }
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Min(10),
        Constraint::Length(3),
    ])
    .split(area);
    let header = Line::from(vec![
        Span::styled(
            " TELL-TAIL ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            " {} · {:?} · {}s window{}",
            s.source.escape_debug(),
            s.state,
            s.window.configured_seconds,
            if frozen { " · DISPLAY FROZEN" } else { "" }
        )),
    ]);
    frame.render_widget(Paragraph::new(header), regions[0]);
    let cards = Layout::horizontal([Constraint::Percentage(25); 4]).split(regions[1]);
    for (index, title, value, color) in [
        (
            0,
            " Entries / sec ",
            format!("{:.0}", s.window.records_per_second),
            accent,
        ),
        (
            1,
            " MiB / sec ",
            format!("{:.2}", s.window.bytes_per_second / 1_048_576.0),
            accent,
        ),
        (
            2,
            " Warnings ",
            percent(s.window.warning_ratio),
            Color::Yellow,
        ),
        (
            3,
            " Errors + fatal ",
            percent(s.window.error_ratio),
            Color::Red,
        ),
    ] {
        frame.render_widget(
            Paragraph::new(value)
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL).title(title)),
            cards[index],
        );
    }
    frame.render_widget(
        Sparkline::default()
            .data(&s.history)
            .style(Style::default().fg(accent))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Throughput · completed seconds "),
            ),
        regions[2],
    );
    let rows = Level::ALL.into_iter().map(|level| {
        let count = s.window.counts.levels.get(level);
        let ratio =
            (s.window.counts.records != 0).then(|| count as f64 / s.window.counts.records as f64);
        let color = match level {
            Level::Warn => Color::Yellow,
            Level::Error | Level::Fatal => Color::Red,
            Level::Unknown => Color::Magenta,
            _ => fg,
        };
        Row::new(vec![
            Cell::from(level.name().to_uppercase()),
            Cell::from(count.to_string()),
            Cell::from(percent(ratio)),
            Cell::from(s.totals.levels.get(level).to_string()),
        ])
        .style(Style::default().fg(color))
    });
    let table = Table::new(rows, [Constraint::Percentage(25); 4])
        .header(
            Row::new(["Severity", "Window count", "Window share", "Session count"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Severity · unknown records remain in the denominator "),
        );
    frame.render_widget(table, regions[3]);
    let backlog = s
        .backlog_bytes
        .map(|b| format!("{b} B"))
        .unwrap_or_else(|| "unknown".into());
    let footer = if help {
        "q: quit · Ctrl-C: interrupt · space: freeze display (ingestion continues)\nJSON uses a top-level level field. Physical lines are records.\nRates use processing time. ? or Esc closes this help.".into()
    } else {
        format!(
            " Session: {} records · {:.2} MiB · {:.1}s   Backlog: {}\n Parse failures: {} · oversized: {} · pending: {} B · skipped: {} B\n q quit · space freeze display · ? help",
            s.totals.records,
            s.bytes_read as f64 / 1_048_576.0,
            s.elapsed_seconds,
            backlog,
            s.totals.parse_failures,
            s.totals.oversized_records,
            s.pending_bytes,
            s.skipped_bytes
        )
    };
    frame.render_widget(Paragraph::new(footer), regions[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parse::Classification,
        stats::{State, Stats},
    };
    use ratatui::backend::TestBackend;
    #[test]
    fn dashboard_contains_counts_and_handles_small_terminals() {
        let mut stats = Stats::new(60, "app.log".into());
        stats.record(
            Classification {
                level: Level::Error,
                parse_failure: false,
            },
            false,
            Duration::ZERO,
        );
        let s = stats.snapshot(Duration::from_secs(1), State::Following, 4, Some(5));
        for (width, height) in [(100, 26), (60, 22), (20, 5), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &s, true, false)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            if width >= 60 {
                assert!(text.contains("100.00%"));
                assert!(text.contains("ERROR"));
            }
        }
    }
}
