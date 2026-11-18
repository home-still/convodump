//! A failure that maps to a process exit code and a what / why / fix message.

use std::sync::Arc;

use hs_common::exit_codes::{GENERAL_ERROR, NETWORK_ERROR, USAGE_ERROR};
use hs_common::reporter::Reporter;

#[derive(Debug, Clone)]
pub struct ExitError {
    pub code: u8,
    pub what: String,
    pub why: Option<String>,
    pub fix: Option<String>,
}

impl ExitError {
    pub fn general(what: impl Into<String>) -> Self {
        Self::new(GENERAL_ERROR, what)
    }

    pub fn usage(what: impl Into<String>) -> Self {
        Self::new(USAGE_ERROR, what)
    }

    pub fn network(what: impl Into<String>) -> Self {
        Self::new(NETWORK_ERROR, what)
    }

    pub fn new(code: u8, what: impl Into<String>) -> Self {
        Self {
            code,
            what: what.into(),
            why: None,
            fix: None,
        }
    }

    pub fn why(mut self, why: impl Into<String>) -> Self {
        self.why = Some(why.into());
        self
    }

    pub fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    pub fn report(&self, reporter: &Arc<dyn Reporter>) {
        reporter.error(&self.what);
        if let Some(why) = &self.why {
            reporter.status("cause", why);
        }
        if let Some(fix) = &self.fix {
            reporter.status("fix", fix);
        }
    }
}

impl std::fmt::Display for ExitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.what)
    }
}

impl std::error::Error for ExitError {}
