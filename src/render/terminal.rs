//! Terminal lifecycle and input polling.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::Event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use keel_core::KernelError;
use tokio::sync::mpsc;

/// Raw mode and the alternate screen, restored on drop, panic and `Ctrl-C`.
pub struct TerminalGuard;

pub fn enter() -> Result<TerminalGuard, KernelError> {
    enable_raw_mode().map_err(io_error)?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(io_error)?;
    Ok(TerminalGuard)
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn io_error(error: io::Error) -> KernelError {
    KernelError::Io {
        message: error.to_string(),
    }
}

/// Polls the terminal on a thread so the async loop can select on keys.
pub fn spawn_key_reader(tx: mpsc::Sender<Event>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match crossterm::event::poll(Duration::from_millis(100)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(event) => {
                        if tx.blocking_send(event).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
}
