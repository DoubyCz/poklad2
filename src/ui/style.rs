//! Barvy v terminálu. Vycházejí z barev originálu (INK/PAPER), ale nechávají
//! pozadí a základní barvu textu na tématu terminálu.
//!
//! Kde terminál umí true color, posílají se barvy ZX Spectra přímo jako RGB.
//! Barvy palety (ANSI 30-37) totiž určuje téma terminálu a třeba černobílé
//! téma Omarchy má místo žluté šedou.

use std::io::IsTerminal;

/// Barvy ZX Spectra (INK) s jasem BRIGHT nebo bez něj.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Green,
    Yellow,
    Magenta,
    Cyan,
    White,
}

impl Ink {
    pub const fn rgb(self, bright: bool) -> (u8, u8, u8) {
        let on = if bright { 0xFF } else { 0xD7 };
        match self {
            Self::Green => (0, on, 0),
            Self::Yellow => (on, on, 0),
            Self::Magenta => (on, 0, on),
            Self::Cyan => (0, on, on),
            Self::White => (on, on, on),
        }
    }

    /// Náhradní barva z palety terminálu (ANSI 30-37).
    pub const fn ansi(self) -> u8 {
        match self {
            Self::Green => 32,
            Self::Yellow => 33,
            Self::Magenta => 35,
            Self::Cyan => 36,
            Self::White => 37,
        }
    }

    /// SGR parametry pro barvu písma.
    pub fn sgr(self, bright: bool, truecolor: bool) -> String {
        if truecolor {
            let (r, g, b) = self.rgb(bright);
            format!("38;2;{r};{g};{b}")
        } else {
            self.ansi().to_string()
        }
    }
}

/// Umí terminál barvy jako RGB? (`COLORTERM=truecolor` nebo `24bit`)
pub fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}

#[derive(Clone, Copy, Debug)]
pub enum Style {
    /// Úvodní obrazovka (originál: žlutá, BRIGHT).
    Title,
    /// Odpověď hry na příkaz (originál: INK 5 na PAPER 1).
    Message,
    /// Výzva k zadání (originál: INK 3).
    Prompt,
    /// "Jsi ..." a popis místnosti.
    Location,
    /// Předměty v místnosti.
    Item,
    /// Nápověda POMOC (originál: INK 7).
    Hint,
    /// Smrt a otázka na novou hru (originál: INK 4).
    Death,
    /// Výhra.
    Win,
    /// Doplňky přepisu, které v originálu nejsou (mapa, poznámky).
    Extra,
}

impl Style {
    /// Atributy (tučně, tlumeně, blikání) a barva písma.
    const fn parts(self) -> (&'static str, Option<(Ink, bool)>) {
        match self {
            Self::Title => ("1", Some((Ink::Yellow, true))),
            Self::Message => ("", Some((Ink::Cyan, false))),
            Self::Prompt => ("", Some((Ink::Magenta, false))),
            Self::Location => ("1", None),
            Self::Item => ("", Some((Ink::Yellow, false))),
            Self::Hint => ("1", Some((Ink::White, true))),
            Self::Death => ("", Some((Ink::Green, false))),
            Self::Win => ("1;5", Some((Ink::Yellow, true))),
            Self::Extra => ("2", None),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    enabled: bool,
    truecolor: bool,
}

impl Palette {
    /// Barvy jen do terminálu a jen když je uživatel nevypnul (`NO_COLOR`).
    pub fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Self { enabled: std::io::stdout().is_terminal() && !no_color, truecolor: truecolor() }
    }

    #[cfg(test)]
    pub const fn plain() -> Self {
        Self { enabled: false, truecolor: false }
    }

    pub fn paint(self, text: &str, style: Style) -> String {
        if !self.enabled {
            return text.to_owned();
        }
        let (attributes, ink) = style.parts();
        let mut codes: Vec<String> = Vec::new();
        if !attributes.is_empty() {
            codes.push(attributes.to_owned());
        }
        if let Some((ink, bright)) = ink {
            codes.push(ink.sgr(bright, self.truecolor));
        }
        format!("\x1b[{}m{text}\x1b[0m", codes.join(";"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_sends_spectrum_rgb() {
        let palette = Palette { enabled: true, truecolor: true };
        assert_eq!(palette.paint("x", Style::Item), "\x1b[38;2;215;215;0mx\x1b[0m");
        let palette = Palette { enabled: true, truecolor: false };
        assert_eq!(palette.paint("x", Style::Title), "\x1b[1;33mx\x1b[0m");
    }
}
