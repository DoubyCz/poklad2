//! Poklad 2 - volný přepis textové adventury Františka Fuky (FUXOFT 1985)
//! ze ZX Spectra do Rustu.

mod data;
mod game;
mod parser;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    match ui::run_terminal() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Chyba: {err}");
            ExitCode::FAILURE
        }
    }
}
