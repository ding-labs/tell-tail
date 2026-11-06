use super::{Classification, Level};

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum ParserKind {
    Text,
    Json,
    Logfmt,
    Profile,
    Apple,
    Syslog,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParseIssue {
    #[default]
    None,
    Numeric,
    Conflicting,
}
#[derive(Clone, Copy, Debug)]
pub struct Assessment {
    pub classification: Classification,
    pub parser: ParserKind,
    pub issue: ParseIssue,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ParsingDiagnostics {
    /// Recognized records by text, JSON, logfmt, profile, Apple, and syslog rules.
    pub recognized: [u64; 6],
    pub numeric: u64,
    pub conflicting: u64,
}
impl ParsingDiagnostics {
    pub fn record(&mut self, assessment: Assessment) {
        let class = assessment.classification;
        if class.level != Level::Unknown {
            self.recognized[assessment.parser as usize] += 1;
        } else if !class.parse_failure {
            match assessment.issue {
                ParseIssue::Numeric => self.numeric += 1,
                ParseIssue::Conflicting => self.conflicting += 1,
                ParseIssue::None => (),
            }
        }
    }
    pub fn hint(&self) -> &'static str {
        if self.conflicting > 0 {
            "Conflicting levels: select --level-field or --level-path explicitly."
        } else if self.numeric > 0 {
            "Numeric levels: select/check --profile pino, python, or otel; mappings differ."
        } else {
            "Unknowns: check --level-field / --level-path; message text is not severity."
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Selection {
    pub level: Option<Level>,
    pub issue: ParseIssue,
}
impl Selection {
    pub fn classification(self) -> Classification {
        Classification {
            level: self.level.unwrap_or(Level::Unknown),
            parse_failure: false,
        }
    }
}
