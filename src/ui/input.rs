//! Zdroj vstupu pro konzoli: celoobrazovkové rozhraní (`tui`), nebo obyčejné
//! čtení řádků ze `BufRead` (roura, testy).

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use super::style::{Palette, Style};
use crate::data::{Dir, Item};
use crate::parser;

/// Co hráč právě vidí a nese, pro tlačítka a doplňování, a mapa.
#[derive(Clone, Default)]
pub struct Scene {
    pub here: Vec<Item>,
    pub carried: Vec<Item>,
    /// Popis místa, kde hráč stojí ("Jsi v lese." a zvláštnosti místnosti).
    pub location: String,
    /// Kolik hodin hráč bloudí a kolik má pokladů (jako INVENTURA).
    pub turns: u32,
    pub treasures: u8,
    /// Kudy z místnosti vede cesta (bez zdi). Zablokovaná cesta ("Prozatím
    /// tam nemůžeš") sem patří taky, zeď to není.
    pub exits: Vec<Dir>,
    /// Vede odtud jáma nebo studna nahoru / dolů?
    pub up: bool,
    pub down: bool,
    /// Titulek mapy patra, kde hráč je.
    pub map_title: String,
    /// Mapa každého prozkoumaného patra zvlášť, po snímcích: věci
    /// v místnostech se střídají po sekundě.
    pub map_frames: HashMap<u8, Vec<String>>,
    /// Patro, kde hráč je.
    pub player_level: u8,
    /// Místnosti, odkud vede prošlé spojení do jiných pater (pro klik v mapě).
    pub portals: HashMap<u8, Vec<u8>>,
    /// Rada pro místnost (co by řekl příkaz POMOC).
    pub hint: String,
    /// Text bubliny u postavy ("Vidím studnu." a rada), jen při prvním
    /// vstupu do místnosti.
    pub bubble: Option<String>,
    /// Jsou dveře odemčené (mění se jejich název)?
    pub door_unlocked: bool,
}

impl Scene {
    /// Název předmětu, jak ho hra vypisuje (odemčené dveře se jmenují jinak).
    pub const fn name(&self, item: Item) -> &'static str {
        match item {
            Item::Door if self.door_unlocked => crate::game::UNLOCKED_DOOR,
            _ => item.name(),
        }
    }
}

pub trait LineReader {
    /// Přečte řádek bez okrajových mezer. `None` = konec vstupu (Ctrl+D).
    /// Ctrl+C vrací chybu `Interrupted`.
    fn read_line(&mut self, prompt: &str, out: &mut dyn Write) -> io::Result<Option<String>>;

    /// Počká na jednu klávesu (jako `INKEY$` v originálu). `None` = konec vstupu.
    /// Bez terminálu se čte celý řádek a platí jeho první znak.
    fn read_key(&mut self, prompt: &str, out: &mut dyn Write) -> io::Result<Option<char>> {
        Ok(self.read_line(prompt, out)?.map(|line| line.chars().next().unwrap_or('\n')))
    }

    fn set_scene(&mut self, _scene: Scene) {}

    /// Konec programu: nechat hráče dočíst poslední výpis.
    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct PlainReader<R> {
    input: R,
    palette: Palette,
}

impl<R> PlainReader<R> {
    pub const fn new(input: R, palette: Palette) -> Self {
        Self { input, palette }
    }
}

impl<R: BufRead> LineReader for PlainReader<R> {
    fn read_line(&mut self, prompt: &str, out: &mut dyn Write) -> io::Result<Option<String>> {
        write!(out, "{}", self.palette.paint(prompt, Style::Prompt))?;
        out.flush()?;
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            writeln!(out)?;
            return Ok(None);
        }
        Ok(Some(line.trim().to_owned()))
    }
}

/// Slova nabízená tabulátorem na začátku řádku. Tajná slova (ALLAH,
/// BALALAJKA) tu záměrně nejsou, hráč je musí najít ve hře.
pub const COMMAND_WORDS: [&str; 17] = [
    "sever", "jih", "západ", "východ", "nahoru", "dolů", "vezmi", "polož", "zastřel",
    "inventura", "rozsviť", "pomoc", "konec", "save", "load", "mapa", "návod",
];

/// Odpovídá `typed` začátku textu nebo začátku některého jeho slova?
pub fn matches(candidate: &str, typed: &str) -> bool {
    let candidate = parser::normalize(candidate);
    let typed = parser::normalize(typed);
    candidate.starts_with(&typed) || candidate.split(' ').any(|word| word.starts_with(&typed))
}

/// Doplnění tabulátorem: vrátí začátek nahrazované části řádku a kandidáty.
pub fn completions(line: &str, scene: &Scene) -> (usize, Vec<String>) {
    match line.find(' ') {
        None => (
            0,
            COMMAND_WORDS.iter().filter(|w| matches(w, line)).map(|w| (*w).to_owned()).collect(),
        ),
        Some(space) => {
            let start = space + line[space..].len() - line[space..].trim_start().len();
            let typed = &line[start..];
            let names = scene.here.iter().chain(&scene.carried).map(|&item| scene.name(item));
            (start, names.filter(|name| matches(name, typed)).map(str::to_owned).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_ignores_diacritics_and_matches_any_word() {
        assert!(matches("rozsviť", "rozsvit"));
        assert!(matches("pytlík zlaťáků", "zla"));
        assert!(matches("stříbrnou lžičku", "lzi"));
        assert!(!matches("diamant", "x"));
    }

    #[test]
    fn completes_commands_then_items() {
        let scene = Scene { here: vec![Item::Lamp], carried: vec![Item::Rifle], ..Scene::default() };
        assert_eq!(completions("ve", &scene), (0, vec!["vezmi".to_owned()]));
        assert_eq!(completions("vezmi  sv", &scene), (7, vec!["zhasnutou svítilnu".to_owned()]));
        assert_eq!(completions("polož p", &scene).1, vec!["pušku".to_owned()]);
    }
}
