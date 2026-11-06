//! One answer on stdout, for scripts and pipes.

use keel_core::TurnState;

use crate::engine::TurnReport;

/// Prints the answer and returns the process exit code.
pub fn render(report: &TurnReport) -> u8 {
    if !report.answer.is_empty() {
        println!("{}", report.answer);
    }
    if report.state == TurnState::Done {
        return 0;
    }
    let message = report
        .error
        .clone()
        .unwrap_or_else(|| format!("turn ended in {}", report.state));
    eprintln!("keelson: {message}");
    1
}
