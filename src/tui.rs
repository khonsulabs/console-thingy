// use std::io::stdin;

// use crossterm::tty::IsTty;

use crate::ConsoleHandle;

pub fn is_tty() -> bool {
    false
    // stdin().is_tty()
}

pub(crate) fn run(_console: ConsoleHandle) -> ! {
    todo!("implement tui version")
}
