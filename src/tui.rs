mod theme;

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
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Sparkline, Table},
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
    let mut review = Review::default();
    loop {
        let current = running.snapshot();
        terminal.draw(|frame| {
            draw_review(
                frame,
                frozen.as_ref().unwrap_or(&current),
                frozen.is_some(),
                help,
                &review,
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
                        review.offset = 0;
                        frozen = if frozen.is_some() {
                            None
                        } else {
                            Some(current)
                        }
                    }
                    KeyCode::Char('?') => help = !help,
                    KeyCode::Esc => help = false,
                    KeyCode::Char('f') => {
                        review.fatal_only = !review.fatal_only;
                        review.offset = 0;
                    }
                    KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End => {
                        let snapshot = frozen.get_or_insert(current);
                        review.scroll(key.code, snapshot);
                    }
                    _ => (),
                }
            }
        }
    }
}

#[derive(Default)]
struct Review {
    fatal_only: bool,
    offset: usize,
}

impl Review {
    fn entries<'a>(&self, s: &'a Snapshot) -> Vec<&'a crate::recent::RecentError> {
        s.recent_errors
            .iter()
            .filter(|e| !self.fatal_only || e.level == Level::Fatal)
            .collect()
    }

    fn scroll(&mut self, key: KeyCode, s: &Snapshot) {
        self.offset = match key {
            KeyCode::Down => self.offset.saturating_add(1),
            KeyCode::PageDown => self.offset.saturating_add(5),
            KeyCode::Up => self.offset.saturating_sub(1),
            KeyCode::PageUp => self.offset.saturating_sub(5),
            KeyCode::End => usize::MAX,
            KeyCode::Home => 0,
            _ => self.offset,
        }
        .min(self.entries(s).len().saturating_sub(1));
    }
}

pub fn draw(frame: &mut Frame, s: &Snapshot, frozen: bool, help: bool) {
    draw_review(frame, s, frozen, help, &Review::default());
}

fn panel<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(theme::TEXT).bg(theme::SURFACE))
        .border_style(Style::default().fg(theme::BORDER))
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title(title.into())
}

fn draw_review(frame: &mut Frame, s: &Snapshot, frozen: bool, help: bool, review: &Review) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().fg(theme::TEXT).bg(theme::CANVAS)),
        area,
    );
    if area.width < 60 || area.height < 18 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        "tell-tail",
                        Style::default()
                            .fg(theme::BRAND)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" · {:?}", s.state),
                        Style::default().fg(theme::state(s.state)),
                    ),
                ]),
                Line::raw(format!(
                    "{} records · {} bytes",
                    s.totals.records, s.bytes_read
                )),
                Line::from(vec![
                    Span::styled(
                        format!("warn {}", percent(s.window.warning_ratio)),
                        Style::default().fg(theme::WARNING),
                    ),
                    Span::raw(" · "),
                    Span::styled(
                        format!("error {}", percent(s.window.error_ratio)),
                        Style::default().fg(theme::INCIDENT),
                    ),
                ]),
                Line::styled(
                    format!("Unknown: {}", s.totals.levels.get(Level::Unknown)),
                    Style::default().fg(theme::INFO),
                ),
                Line::raw(format!("Parse failures: {}", s.totals.parse_failures)),
                Line::styled(
                    "Resize to 60 × 18 for dashboard",
                    Style::default().fg(theme::MUTED),
                ),
                Line::styled("q quit · space freeze", Style::default().fg(theme::MUTED)),
            ]),
            area,
        );
        return;
    }
    let review_height = if area.height >= 20 {
        area.height - 17
    } else {
        0
    };
    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(review_height),
        Constraint::Length(3),
    ])
    .split(area);
    let header = Line::from(vec![
        Span::styled(
            " ≡╱ ",
            Style::default()
                .fg(theme::BRAND)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "tell-tail",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {:?}", s.state),
            Style::default().fg(theme::state(s.state)),
        ),
        Span::styled(
            if frozen { " · DISPLAY FROZEN" } else { "" },
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}s · ", s.window.configured_seconds),
            Style::default().fg(theme::MUTED),
        ),
        Span::raw(s.source.escape_debug().to_string()),
    ]);
    frame.render_widget(Paragraph::new(header), regions[0]);
    let cards = Layout::horizontal([Constraint::Percentage(25); 4]).split(regions[1]);
    for (index, title, value, color) in [
        (
            0,
            " Entries / sec ",
            format!("{:.0}", s.window.records_per_second),
            theme::BRAND,
        ),
        (
            1,
            " MiB / sec ",
            format!("{:.2}", s.window.bytes_per_second / 1_048_576.0),
            theme::TEXT,
        ),
        (
            2,
            " Warnings ",
            percent(s.window.warning_ratio),
            theme::WARNING,
        ),
        (
            3,
            " Errors + fatal ",
            percent(s.window.error_ratio),
            theme::INCIDENT,
        ),
    ] {
        frame.render_widget(
            Paragraph::new(value)
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .block(panel(title)),
            cards[index],
        );
    }
    let overview =
        Layout::horizontal([Constraint::Min(16), Constraint::Length(43)]).split(regions[2]);
    let history_start = s
        .history
        .len()
        .saturating_sub(usize::from(overview[0].width.saturating_sub(2)));
    frame.render_widget(
        Sparkline::default()
            .data(&s.history[history_start..])
            .style(Style::default().fg(theme::BRAND))
            .block(panel(" Throughput · entries/s ")),
        overview[0],
    );
    let rows = Level::ALL.into_iter().map(|level| {
        let count = s.window.counts.levels.get(level);
        let ratio =
            (s.window.counts.records != 0).then(|| count as f64 / s.window.counts.records as f64);
        Row::new(vec![
            Cell::from(level.name().to_uppercase()),
            Cell::from(count.to_string()),
            Cell::from(percent(ratio)),
            Cell::from(s.totals.levels.get(level).to_string()),
        ])
        .style(Style::default().fg(theme::severity(level)))
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Min(8),
            Constraint::Length(9),
            Constraint::Min(8),
        ],
    )
    .header(
        Row::new(["Level", "Window", "Share", "Session"]).style(
            Style::default()
                .fg(theme::MUTED)
                .bg(theme::SUBTLE)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(panel(" Severity "));
    frame.render_widget(table, overview[1]);
    let entries = review.entries(s);
    let offset = review.offset.min(entries.len().saturating_sub(1));
    let visible = usize::from(regions[3].height.saturating_sub(2));
    let title = format!(
        " Recent {} · {}/{} · newest first ",
        if review.fatal_only {
            "fatals"
        } else {
            "errors + fatal"
        },
        (offset + visible).min(entries.len()),
        entries.len(),
    );
    let mut lines: Vec<Line> = entries
        .iter()
        .skip(offset)
        .take(visible)
        .map(|entry| {
            let prefix = format!(
                "{:>6.1}s {:5} ",
                entry.elapsed_seconds,
                entry.level.name().to_uppercase()
            );
            let width =
                usize::from(regions[3].width.saturating_sub(2)).saturating_sub(prefix.len());
            let mut preview = Line::from(entry.preview.to_string());
            let clipped = preview.width() > width || entry.truncated;
            if clipped {
                let mut value = String::new();
                for ch in entry.preview.chars() {
                    let next = format!("{value}{ch}");
                    if Line::from(next.as_str()).width() >= width {
                        break;
                    }
                    value.push(ch);
                }
                value.push('…');
                preview = Line::from(value);
            }
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme::severity(entry.level))),
                Span::raw(preview.to_string()),
            ])
        })
        .collect();
    if entries.is_empty() {
        lines.push(Line::styled(
            if review.fatal_only {
                "No fatal records captured. f shows errors too."
            } else {
                "No errors or fatal records captured."
            },
            Style::default().fg(theme::MUTED),
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(panel(title)), regions[3]);
    let backlog = s
        .backlog_bytes
        .map(|b| format!("{b} B"))
        .unwrap_or_else(|| "unknown".into());
    let footer = if help {
        "q: quit · Ctrl-C: interrupt · space: freeze display (ingestion continues)\n↑/↓ or PgUp/PgDn review (freezes display) · f fatal only\n20 previews, 512 bytes each · ? or Esc closes help".into()
    } else {
        format!(
            " Session: {} records · {:.2} MiB · {:.1}s   Backlog: {}\n Parse failures: {} · oversized: {} · pending: {} B · skipped: {} B\n q quit · space freeze · ↑↓ review · f fatal · ? help",
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
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
        regions[4],
    );
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
    fn narrow_chart_keeps_the_newest_completed_second() {
        let mut snapshot = Stats::new(60, "stdin".into()).snapshot(
            Duration::from_secs(60),
            State::Following,
            0,
            None,
        );
        snapshot.history = vec![0; 60];
        snapshot.history[59] = 100;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &snapshot, false, false)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(35, 12)].symbol(), "█");
        assert_eq!(buffer[(1, 12)].symbol(), " ");
    }

    #[test]
    fn recent_errors_can_be_filtered_and_scrolled_without_losing_metrics() {
        use clap::Parser;
        let mut engine = crate::engine::Engine::new(&crate::cli::Args::parse_from([
            "tell-tail",
            "--output",
            "tui",
        ]));
        engine.feed(
            b"ERROR first\nFATAL second\nERROR third\n",
            Duration::from_secs(1),
        );
        let snapshot = engine.snapshot(Duration::from_secs(2), State::Following, None);
        let mut review = Review::default();
        for (width, height) in [(60, 20), (80, 24), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| draw_review(f, &snapshot, true, false, &review))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("ERROR third"), "{width}x{height}: {text}");
            assert!(text.contains("UNKNOWN"));
            assert!(text.contains("DISPLAY FROZEN"));
        }
        review.scroll(KeyCode::End, &snapshot);
        assert_eq!(
            review.entries(&snapshot)[review.offset].preview.as_ref(),
            "ERROR first"
        );
        review.scroll(KeyCode::Home, &snapshot);
        review.fatal_only = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| draw_review(f, &snapshot, false, false, &review))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("FATAL second"));
        assert!(!text.contains("ERROR third"));
        assert!(!text.contains("ERROR first"));
    }
    #[test]
    fn unknown_and_parse_failures_remain_visible_in_compact_view() {
        let mut stats = Stats::new(60, "stdin".into());
        stats.record(
            Classification {
                level: Level::Unknown,
                parse_failure: true,
            },
            false,
            Duration::ZERO,
        );
        let snapshot = stats.snapshot(Duration::from_secs(1), State::Following, 0, None);
        for (width, height) in [(20, 8), (40, 10), (60, 22), (100, 26)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| draw(frame, &snapshot, false, false))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(
                text.contains("Parse failures: 1"),
                "{width}x{height}: {text}"
            );
            assert!(text.contains(if width < 60 { "Unknown: 1" } else { "UNKNOWN" }));
        }
    }

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
        for (width, height) in [(120, 40), (100, 26), (80, 24), (60, 18), (20, 5), (1, 1)] {
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
