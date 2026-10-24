//! Ding's dark palette, matching the embedded dashboard's semantic colors.
use crate::{parse::Level, stats::State};
use ratatui::style::Color;

pub(super) const CANVAS: Color = Color::Rgb(23, 28, 25);
pub(super) const SURFACE: Color = Color::Rgb(32, 39, 34);
pub(super) const SUBTLE: Color = Color::Rgb(41, 49, 43);
pub(super) const TEXT: Color = Color::Rgb(240, 241, 233);
pub(super) const MUTED: Color = Color::Rgb(172, 182, 171);
pub(super) const BORDER: Color = Color::Rgb(59, 71, 60);
pub(super) const BRAND: Color = Color::Rgb(126, 231, 135);
pub(super) const INFO: Color = Color::Rgb(194, 173, 223);
pub(super) const WARNING: Color = Color::Rgb(235, 188, 116);
pub(super) const INCIDENT: Color = Color::Rgb(240, 163, 152);

pub(super) fn severity(level: Level) -> Color {
    match level {
        Level::Trace | Level::Debug => MUTED,
        Level::Info | Level::Unknown => INFO,
        Level::Warn => WARNING,
        Level::Error | Level::Fatal => INCIDENT,
    }
}

pub(super) fn state(state: State) -> Color {
    match state {
        State::Reading | State::Following => INFO,
        State::Waiting => WARNING,
        State::Failed => INCIDENT,
        State::Complete | State::Stopped => MUTED,
    }
}
