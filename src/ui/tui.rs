//! Celoobrazovkové rozhraní v terminálu: vlevo text hry, který se vypisuje
//! jako na psacím stroji, vpravo tlačítka na myš, dole řádek pro psaní.
//! Klikání a psaní jde libovolně kombinovat (klik na VEZMI, dopsat předmět).

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::input::{self, LineReader, Scene};
use super::style::{self as palette, Ink};
use super::map;
use crate::data::Dir;
use crate::parser;

/// Rychlost psacího stroje.
const CHARS_PER_SECOND: f64 = 600.0;

/// Poslední zadaný příkaz (hnědá).
const LATEST_COMMAND: (u8, u8, u8) = (0xC0, 0x7A, 0x30);
/// Starší příkazy (tmavě šedá) a starší text (šedá).
const OLD_COMMAND: (u8, u8, u8) = (0x5C, 0x5C, 0x5C);
const OLD_TEXT: (u8, u8, u8) = (0x9A, 0x9A, 0x9A);

fn truecolor() -> bool {
    static TRUECOLOR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRUECOLOR.get_or_init(palette::truecolor)
}

/// RGB barva, nebo náhradní z palety, když terminál RGB neumí.
fn rgb_or((r, g, b): (u8, u8, u8), fallback: Color) -> Color {
    if truecolor() { Color::Rgb(r, g, b) } else { fallback }
}

/// Barva ZX Spectra pro rozhraní: RGB, kde to terminál umí, jinak z palety
/// terminálu (ta ale podle tématu nemusí být barevná).
fn ink(ink: Ink, bright: bool) -> Color {
    if truecolor() {
        let (r, g, b) = ink.rgb(bright);
        Color::Rgb(r, g, b)
    } else {
        match ink {
            Ink::Green => Color::Green,
            Ink::Yellow => Color::Yellow,
            Ink::Magenta => Color::Magenta,
            Ink::Cyan => Color::Cyan,
            Ink::White => Color::White,
        }
    }
}
/// Šířka panelu s tlačítky (včetně rámečku).
const PANEL_WIDTH: u16 = 34;
/// Pod touto šířkou terminálu se panel s tlačítky nevejde a schová se.
const MIN_WIDTH_FOR_PANEL: u16 = 70;
const SCROLL_STEP: usize = 3;
/// Rychlost psacího stroje v bublině s radou (pomaleji, aby se dala číst).
const BUBBLE_CHARS_PER_SECOND: f64 = 35.0;
/// Nejširší řádek textu v bublině.
const BUBBLE_WIDTH: usize = 30;
const HISTORY_SIZE: usize = 200;

/// Text vypsaný hrou: znaky s barvou, z nichž je zatím vidět `revealed`.
#[derive(Default)]
pub struct Screen {
    cells: Vec<(char, Style)>,
    revealed: usize,
    /// Nedokončený UTF-8 znak nebo escape sekvence z minulého zápisu.
    pending: Vec<u8>,
    style: Style,
    /// Odkud začíná poslední akce (zadání hráče a odpověď hry). Ta je barevně
    /// (příkaz hnědě, odpověď žlutě), starší text šedě.
    latest: usize,
    /// Úseky textu, které napsal hráč (opakované zadání).
    inputs: Vec<std::ops::Range<usize>>,
}

/// Řádek ze znaků, sousední znaky se stejnou barvou v jednom úseku.
fn styled_line(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for chunk in cells.chunk_by(|a, b| a.1 == b.1) {
        spans.push(Span::styled(chunk.iter().map(|&(c, _)| c).collect::<String>(), chunk[0].1));
    }
    Line::from(spans)
}

impl Screen {
    fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(text) => text.len(),
            Err(err) => err.valid_up_to(),
        };
        let text = String::from_utf8_lossy(&self.pending[..valid]).into_owned();
        let mut rest = text.as_str();
        while let Some(esc) = rest.find('\x1b') {
            self.push_text(&rest[..esc]);
            let Some(end) = rest[esc..].find('m') else {
                break;
            };
            self.apply_sgr(&rest[esc + 2..esc + end]);
            rest = &rest[esc + end + 1..];
        }
        if rest.contains('\x1b') {
            let keep = rest.len();
            self.pending.drain(..valid - keep);
        } else {
            self.push_text(rest);
            self.pending.drain(..valid);
        }
    }

    fn push_text(&mut self, text: &str) {
        self.cells.extend(text.chars().filter(|&c| c != '\r').map(|c| (c, self.style)));
    }

    /// Barvy, které posílá `style::Palette` (ANSI SGR, i RGB `38;2;r;g;b`).
    fn apply_sgr(&mut self, params: &str) {
        let mut codes = params.split(';');
        while let Some(code) = codes.next() {
            self.style = match code {
                "0" | "" => Style::new(),
                "1" => self.style.add_modifier(Modifier::BOLD),
                "2" => self.style.add_modifier(Modifier::DIM),
                "5" => self.style.add_modifier(Modifier::SLOW_BLINK),
                "32" => self.style.fg(ink(Ink::Green, false)),
                "33" => self.style.fg(ink(Ink::Yellow, false)),
                "35" => self.style.fg(ink(Ink::Magenta, false)),
                "36" => self.style.fg(ink(Ink::Cyan, false)),
                "37" => self.style.fg(ink(Ink::White, false)),
                "38" => {
                    let rgb: Vec<u8> = codes.by_ref().skip(1).take(3).filter_map(|c| c.parse().ok()).collect();
                    match rgb[..] {
                        [r, g, b] => self.style.fg(Color::Rgb(r, g, b)),
                        _ => self.style,
                    }
                }
                _ => self.style,
            };
        }
    }

    const fn animating(&self) -> bool {
        self.revealed < self.cells.len()
    }

    const fn reveal_all(&mut self) {
        self.revealed = self.cells.len();
    }

    /// Začíná nová akce: dosavadní text se odsune do pozadí.
    const fn start_latest(&mut self) {
        self.latest = self.cells.len();
    }

    /// Zapamatuje si, že znaky od `start` do konce napsal hráč.
    fn mark_input(&mut self, start: usize) {
        self.inputs.push(start..self.cells.len());
    }

    /// Barva znaku podle toho, jestli patří k poslední akci a jestli ho
    /// napsal hráč. Starší text je bez původních barev, aby se starší žluté
    /// předměty nepletly s poslední akcí.
    fn color(&self, index: usize, style: Style) -> Style {
        let typed = self.inputs.iter().any(|range| range.contains(&index));
        match (index >= self.latest, typed) {
            (true, true) => style.fg(rgb_or(LATEST_COMMAND, Color::Indexed(130))),
            (true, false) => style.fg(ink(Ink::Yellow, true)),
            (false, true) => Style::new().fg(rgb_or(OLD_COMMAND, Color::DarkGray)),
            (false, false) => Style::new().fg(rgb_or(OLD_TEXT, Color::Gray)),
        }
    }

    /// Viditelný text zalomený po slovech na `width` znaků.
    fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let width = width.max(1);
        let visible: Vec<(char, Style)> = self.cells[..self.revealed]
            .iter()
            .enumerate()
            .map(|(index, &(c, style))| (c, self.color(index, style)))
            .collect();
        let mut lines = Vec::new();
        for paragraph in visible.split(|&(c, _)| c == '\n') {
            let mut rest = paragraph;
            while rest.len() > width {
                // Zlomit za poslední mezerou, která se vejde; dlouhé slovo natvrdo.
                let cut = rest[..=width].iter().rposition(|&(c, _)| c == ' ').filter(|&i| i > 0);
                let (row, next) = match cut {
                    Some(space) => (&rest[..space], &rest[space + 1..]),
                    None => rest.split_at(width),
                };
                lines.push(styled_line(row));
                rest = next;
            }
            lines.push(styled_line(rest));
        }
        lines
    }
}

/// Výstup konzole: místo na terminál píše do sdílené `Screen`.
pub struct ScreenWriter(pub Rc<RefCell<Screen>>);

impl Write for ScreenWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().feed(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Here,
    Carried,
}

#[derive(Clone, PartialEq, Eq)]
enum Button {
    /// Provede příkaz rovnou.
    Run(&'static str),
    /// Napíše začátek příkazu, předmět se doklikne nebo dopíše.
    Start(&'static str),
    Item(String, Source),
    Key(char),
}

/// Bublina s radou u postavy, která se objeví při vstupu do místnosti
/// s radou jinou než obecnou (Jirkovo přání).
struct Bubble {
    text: String,
    started: Instant,
}

impl Bubble {
    /// Kolik znaků už psací stroj napsal.
    fn revealed(&self) -> usize {
        // Na zlomcích znaku nezáleží, jde jen o rychlost animace.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let shown = (self.started.elapsed().as_secs_f64() * BUBBLE_CHARS_PER_SECOND) as usize;
        shown.min(self.text.chars().count())
    }

    fn writing(&self) -> bool {
        self.revealed() < self.text.chars().count()
    }

    /// Dopsat celý text hned.
    fn finish(&mut self) {
        self.started = Instant::now().checked_sub(Duration::from_secs(3600)).unwrap_or(self.started);
    }
}

/// Mapa v hlavní ploše (po úvodu): kam je odrolovaná.
#[derive(Default)]
struct BigMap {
    /// Odrolování (řádek, sloupec) levého horního rohu.
    row: usize,
    col: usize,
    /// Při příštím vykreslení vystředit na hráče.
    center: bool,
    /// Prohlížené jiné patro (klikem na spojení), jinak patro hráče.
    viewed: Option<u8>,
    /// Kde na obrazovce začíná řádek 0 a sloupec 0 mapy (pro klik).
    origin: (i32, i32),
}

/// Na co se právě čeká.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Herní příkaz.
    Command,
    /// Jiná odpověď na řádek (předmět, jméno pozice).
    Answer,
    /// Otázka A/N.
    YesNo,
    /// "Stiskni libovolné tlačítko".
    AnyKey,
}

enum Outcome {
    Line(String),
    Key(char),
    Eof,
}

pub struct Tui {
    terminal: DefaultTerminal,
    screen: Rc<RefCell<Screen>>,
    scene: Scene,
    input: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    /// O kolik řádků je text odrolovaný nahoru od konce.
    scroll: usize,
    buttons: Vec<(Rect, Button)>,
    hover: Option<Position>,
    status: String,
    last_tick: Instant,
    big_map: BigMap,
    /// Kde je text hry a kde mapa (pro kolečko myši).
    text_area: Rect,
    map_area: Rect,
    /// Po úvodu je v hlavní ploše mapa a text hry v postranním panelu.
    map_mode: bool,
    /// Od kdy běží rozhraní (věci v mapě se střídají po sekundě).
    started: Instant,
    /// Píše hráč do řádku? Jinak klávesy ovládají pohyb (Jirkovo přání:
    /// řádek je aktivní, jen když do něj hráč klikne nebo stiskne Enter).
    typing: bool,
    input_area: Rect,
    bubble: Option<Bubble>,
}

/// Co znamená klávesa, když hráč nepíše do řádku.
enum Movement {
    /// Klávesa pohyb není.
    None,
    /// Pohyb směrem do zdi: nic se nestane (jako šedé tlačítko).
    Blocked,
    Go(&'static str),
}

impl Tui {
    pub fn new(screen: Rc<RefCell<Screen>>) -> io::Result<Self> {
        let terminal = ratatui::try_init()?;
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self {
            terminal,
            screen,
            scene: Scene::default(),
            input: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            scroll: 0,
            buttons: Vec::new(),
            hover: None,
            status: String::new(),
            last_tick: Instant::now(),
            big_map: BigMap::default(),
            text_area: Rect::default(),
            map_area: Rect::default(),
            map_mode: false,
            started: Instant::now(),
            typing: false,
            input_area: Rect::default(),
            bubble: None,
        })
    }

    fn input_text(&self) -> String {
        self.input.iter().collect()
    }

    fn set_input(&mut self, text: &str) {
        self.input = text.chars().collect();
        self.cursor = self.input.len();
    }

    /// Posune psací stroj podle uplynulého času.
    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        let mut screen = self.screen.borrow_mut();
        if screen.animating() {
            // Na zlomcích znaku nezáleží, jde jen o rychlost animace.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let step = ((elapsed * CHARS_PER_SECOND) as usize).max(1);
            screen.revealed = (screen.revealed + step).min(screen.cells.len());
        }
        self.last_tick = now;
    }

    /// Hlavní smyčka: kreslí, animuje a čeká na klávesu nebo kliknutí.
    fn wait(&mut self, prompt: &str, mode: Mode, out: &mut dyn Write) -> io::Result<Outcome> {
        self.scroll = 0;
        self.last_tick = Instant::now();
        loop {
            self.tick();
            self.draw(prompt, mode)?;
            let animating = self.screen.borrow().animating();
            let bubble_writing = self.bubble.as_ref().is_some_and(Bubble::writing);
            let timeout = Duration::from_millis(if animating || bubble_writing { 16 } else { 500 });
            if !event::poll(timeout)? {
                continue;
            }
            let outcome = match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if is_ctrl(key, 'c') {
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                    if animating {
                        self.screen.borrow_mut().reveal_all();
                        // Psaní během animace se neztratí, ostatní klávesy ji jen přeskočí.
                        if mode == Mode::YesNo || mode == Mode::AnyKey || !is_typing(key) {
                            continue;
                        }
                    }
                    self.on_key(key, mode)
                }
                Event::Mouse(mouse) => {
                    if animating && mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                        self.screen.borrow_mut().reveal_all();
                        continue;
                    }
                    self.on_mouse(mouse, mode)
                }
                _ => None,
            };
            if let Some(outcome) = outcome {
                self.echo(prompt, mode, &outcome, out)?;
                // Konec úvodu: od teď je v hlavní ploše mapa.
                if mode == Mode::AnyKey {
                    self.map_mode = true;
                }
                return Ok(outcome);
            }
        }
    }

    /// Zopakuje zadání do textu hry, jako by se psalo na obrazovku.
    fn echo(&mut self, prompt: &str, mode: Mode, outcome: &Outcome, out: &mut dyn Write) -> io::Result<()> {
        // Nový herní příkaz (nebo konec úvodu) začíná novou akci. Odpovědi
        // na doplňující otázky (Co ?, A/N) patří ještě k té předchozí.
        out.flush()?;
        if matches!(mode, Mode::Command | Mode::AnyKey) {
            self.screen.borrow_mut().start_latest();
        }
        let start = self.screen.borrow().cells.len();
        match (mode, outcome) {
            (Mode::Command | Mode::Answer, Outcome::Line(line)) => writeln!(out, "{prompt}{line}")?,
            (Mode::YesNo, Outcome::Key(key)) => writeln!(out, "{}", key.to_uppercase())?,
            _ => {}
        }
        out.flush()?;
        let mut screen = self.screen.borrow_mut();
        screen.mark_input(start);
        screen.reveal_all();
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent, mode: Mode) -> Option<Outcome> {
        if is_ctrl(key, 'd') && (self.input.is_empty() || matches!(mode, Mode::YesNo | Mode::AnyKey)) {
            return Some(Outcome::Eof);
        }
        match mode {
            Mode::YesNo | Mode::AnyKey => match key.code {
                KeyCode::Char(c) => Some(Outcome::Key(c)),
                KeyCode::Enter => Some(Outcome::Key('\n')),
                _ if mode == Mode::AnyKey => Some(Outcome::Key(' ')),
                _ => None,
            },
            Mode::Command if !self.typing => {
                // Bublinu s radou zavře Enter, mezerník nebo Esc (rozepsanou
                // nejdřív dopíše).
                if let Some(bubble) = &mut self.bubble
                    && matches!(key.code, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc)
                {
                    if bubble.writing() {
                        bubble.finish();
                    } else {
                        self.bubble = None;
                    }
                    return None;
                }
                // Enter nebo Tab přepne z mapy do řádku pro psaní.
                if matches!(key.code, KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab) {
                    self.typing = true;
                    return None;
                }
                // Esc vrátí mapu z prohlíženého patra na patro hráče.
                if key.code == KeyCode::Esc {
                    self.big_map.viewed = None;
                    self.big_map.center = true;
                    return None;
                }
                match self.movement(key) {
                    Movement::Go(command) => Some(Outcome::Line(command.to_owned())),
                    Movement::Blocked | Movement::None => {
                        self.scroll_text(key);
                        None
                    }
                }
            }
            // Tab v prázdném řádku (a Shift+Tab vždy) přepne zpět do mapy,
            // v rozepsaném řádku Tab doplňuje.
            Mode::Command
                if key.code == KeyCode::BackTab || (key.code == KeyCode::Tab && self.input_text().trim().is_empty()) =>
            {
                self.set_input("");
                self.typing = false;
                None
            }
            Mode::Command | Mode::Answer => self.edit_line(key),
        }
    }

    /// Pohyb klávesami, když hráč nepíše: šipky nebo W/S/A/D = světové
    /// strany, R/F nebo PageUp/PageDown = nahoru/dolů po patrech.
    fn movement(&self, key: KeyEvent) -> Movement {
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT) {
            return Movement::None;
        }
        let (open, command) = match key.code {
            KeyCode::Up | KeyCode::Char('w' | 'W') => (self.scene.exits.contains(&Dir::North), "sever"),
            KeyCode::Down | KeyCode::Char('s' | 'S') => (self.scene.exits.contains(&Dir::South), "jih"),
            KeyCode::Left | KeyCode::Char('a' | 'A') => (self.scene.exits.contains(&Dir::West), "západ"),
            KeyCode::Right | KeyCode::Char('d' | 'D') => (self.scene.exits.contains(&Dir::East), "východ"),
            KeyCode::PageUp | KeyCode::Char('r' | 'R') => (self.scene.up, "nahoru"),
            KeyCode::PageDown | KeyCode::Char('f' | 'F') => (self.scene.down, "dolů"),
            _ => return Movement::None,
        };
        if open { Movement::Go(command) } else { Movement::Blocked }
    }

    /// Shift+PageUp/PageDown roluje text hry (i když hráč nepíše).
    fn scroll_text(&mut self, key: KeyEvent) {
        if !key.modifiers.contains(KeyModifiers::SHIFT) {
            return;
        }
        match key.code {
            KeyCode::PageUp => self.scroll += SCROLL_STEP * 4,
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(SCROLL_STEP * 4),
            _ => {}
        }
    }

    fn edit_line(&mut self, key: KeyEvent) -> Option<Outcome> {
        self.status.clear();
        match key.code {
            KeyCode::Enter => {
                let line = self.input_text().trim().to_owned();
                if !line.is_empty() && self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                    if self.history.len() > HISTORY_SIZE {
                        self.history.remove(0);
                    }
                }
                self.history_pos = None;
                self.set_input("");
                self.typing = false;
                return Some(Outcome::Line(line));
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.input.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.input.len() => {
                self.input.remove(self.cursor);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::Esc => {
                self.set_input("");
                self.typing = false;
            }
            KeyCode::Up => self.browse_history(true),
            KeyCode::Down => self.browse_history(false),
            KeyCode::Tab => self.complete(),
            KeyCode::PageUp | KeyCode::PageDown => self.scroll_text(key),
            _ => {}
        }
        None
    }

    fn browse_history(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.history_pos = match (self.history_pos, older) {
            (None, true) => Some(last),
            (Some(pos), true) => Some(pos.saturating_sub(1)),
            (Some(pos), false) if pos < last => Some(pos + 1),
            (_, false) => None,
        };
        let text = self.history_pos.map_or_else(String::new, |pos| self.history[pos].clone());
        self.set_input(&text);
    }

    fn complete(&mut self) {
        let line = self.input_text();
        let (start, candidates) = input::completions(&line, &self.scene);
        match candidates.as_slice() {
            [] => {}
            [only] => {
                let space = if start == 0 { " " } else { "" };
                self.set_input(&format!("{}{only}{space}", &line[..start]));
            }
            many => self.status = many.join(" · "),
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent, mode: Mode) -> Option<Outcome> {
        let position = Position::new(mouse.column, mouse.row);
        let on_map = self.map_mode && self.map_area.contains(position);
        let sideways = mouse.modifiers.contains(KeyModifiers::SHIFT);
        let map = &mut self.big_map;
        match mouse.kind {
            MouseEventKind::ScrollUp if on_map && sideways => map.col = map.col.saturating_sub(SCROLL_STEP * 2),
            MouseEventKind::ScrollDown if on_map && sideways => map.col += SCROLL_STEP * 2,
            MouseEventKind::ScrollUp if on_map => map.row = map.row.saturating_sub(SCROLL_STEP),
            MouseEventKind::ScrollDown if on_map => map.row += SCROLL_STEP,
            MouseEventKind::ScrollLeft if on_map => map.col = map.col.saturating_sub(SCROLL_STEP * 2),
            MouseEventKind::ScrollRight if on_map => map.col += SCROLL_STEP * 2,
            MouseEventKind::ScrollUp => self.scroll += SCROLL_STEP,
            MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_sub(SCROLL_STEP),
            MouseEventKind::Moved | MouseEventKind::Drag(_) => self.hover = Some(position),
            MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Right)) if on_map => {
                // Klik do mapy vrací klávesy k pohybu. Na spojení pater přepne
                // mapu na druhé patro (levým dolů, pravým nahoru), jinde vystředí.
                self.typing = false;
                let level = map.viewed.unwrap_or(self.scene.player_level);
                let y = i32::from(mouse.row) - map.origin.0;
                let x = i32::from(mouse.column) - map.origin.1;
                let room = usize::try_from(y)
                    .ok()
                    .zip(usize::try_from(x).ok())
                    .and_then(|(y, x)| map::Scale::Large.room_at(level, y, x));
                if let Some(levels) = room.and_then(|room| self.scene.portals.get(&room)) {
                    let down = levels.iter().find(|&&l| l > level);
                    let up = levels.iter().rev().find(|&&l| l < level);
                    let target = if button == MouseButton::Right { up.or(down) } else { down.or(up) };
                    map.viewed = target.copied().filter(|&l| l != self.scene.player_level);
                }
                map.center = true;
                return None;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if mode == Mode::AnyKey {
                    return Some(Outcome::Key(' '));
                }
                if self.input_area.contains(position) && mode == Mode::Command {
                    self.typing = true;
                    return None;
                }
                let button = self.buttons.iter().find(|(rect, _)| rect.contains(position))?.1.clone();
                return self.press(button, mode);
            }
            _ => {}
        }
        None
    }

    fn press(&mut self, button: Button, mode: Mode) -> Option<Outcome> {
        let line = match button {
            Button::Key(key) => return Some(Outcome::Key(key)),
            Button::Start(verb) => {
                self.set_input(&format!("{verb} "));
                self.typing = true;
                return None;
            }
            Button::Run(command) => command.to_owned(),
            Button::Item(name, source) => {
                let typed = self.input_text();
                let verb_word = typed.split_whitespace().next().unwrap_or("");
                let verb = parser::parse(verb_word).map(|command| command.verb);
                if mode == Mode::Answer {
                    name
                } else if matches!(verb, Some(parser::Verb::Take | parser::Verb::Drop)) {
                    // Rozepsaný předmět nahradí ten, na který hráč klikl.
                    format!("{verb_word} {name}")
                } else if source == Source::Here {
                    format!("vezmi {name}")
                } else {
                    format!("polož {name}")
                }
            }
        };
        self.set_input("");
        Some(Outcome::Line(line))
    }

    fn draw(&mut self, prompt: &str, mode: Mode) -> io::Result<()> {
        let mut buttons = Vec::new();
        let screen = Rc::clone(&self.screen);
        let input = self.input_text();
        let cursor = self.cursor;
        let hover = self.hover;
        let status = self.status.clone();
        let scene = self.scene.clone();
        let scroll = &mut self.scroll;
        let big_map = &mut self.big_map;
        let map_mode = self.map_mode && !scene.map_frames.is_empty();
        let level = big_map.viewed.filter(|l| scene.map_frames.contains_key(l)).unwrap_or(scene.player_level);
        let map_title = if level == scene.player_level {
            scene.map_title.clone()
        } else {
            format!(" Mapa: {} · prohlížíš, Esc zpět ", map::level_name(level))
        };
        let second = usize::try_from(self.started.elapsed().as_secs()).unwrap_or(0);
        let (mut text_rect, mut map_rect, mut input_rect) = (Rect::default(), Rect::default(), Rect::default());
        let typing = self.typing || mode == Mode::Answer;
        let bubble = self.bubble.as_ref().map(|b| (b.text.clone(), b.revealed()));
        self.terminal.draw(|frame| {
            let [main, input_area] =
                Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(frame.area());
            let (main_area, panel) = if main.width >= MIN_WIDTH_FOR_PANEL {
                let [main, panel] =
                    Layout::horizontal([Constraint::Min(34), Constraint::Length(PANEL_WIDTH)]).areas(main);
                (main, Some(panel))
            } else {
                (main, None)
            };
            // Po úvodu: mapa v hlavní ploše, text hry v panelu (když panel je).
            let map_in_main = map_mode && panel.is_some();
            if map_in_main {
                map_rect = main_area;
                let frames = scene.map_frames.get(&level).map_or(&[][..], Vec::as_slice);
                let map = frames.get(second % frames.len().max(1)).map_or("", String::as_str);
                let player = draw_big_map(frame, main_area, (&map_title, !typing), map, level, big_map);
                if let (Some((text, revealed)), Some(player)) = (&bubble, player)
                    && level == scene.player_level
                {
                    draw_bubble(frame, main_area, player, text, *revealed);
                }
            } else {
                text_rect = main_area;
                // V úvodu (a bez panelu) je text hlavní okno.
                draw_text(frame, main_area, &screen.borrow(), scroll, !typing);
            }
            if let Some(panel) = panel {
                let mut ui = Buttons { frame, list: &mut buttons, hover };
                if let Some(text_area) = ui.panel(panel, mode, &scene, map_in_main) {
                    text_rect = text_area;
                    draw_text(ui.frame, text_area, &screen.borrow(), scroll, false);
                }
            }
            input_rect = input_area;
            let line = InputLine { prompt, mode, text: &input, cursor, status: &status, typing };
            draw_input(frame, input_area, &line);
        })?;
        self.buttons = buttons;
        self.text_area = text_rect;
        self.map_area = map_rect;
        self.input_area = input_rect;
        Ok(())
    }
}

/// Řádek mapy: `@` zvýrazněný, zdi šedé, zbytek beze změny.
/// `left` mezer odsazení, pak řádek mapy patra `level`. Zdi mají barvu
/// prostředí patra.
fn map_line(text: &str, left: usize, level: u8) -> Line<'static> {
    let here = Style::new().fg(Color::Black).bg(ink(Ink::Yellow, true)).add_modifier(Modifier::BOLD);
    let wall = Style::new().fg(wall_color(level));
    let cells: Vec<(char, Style)> = std::iter::repeat_n((' ', Style::new()), left)
        .chain(text.chars().map(|c| match c {
            '@' => (c, here),
            map::WALL_HORIZONTAL | map::WALL_VERTICAL => (c, wall),
            _ => (c, Style::new()),
        }))
        .collect();
    styled_line(&cells)
}

/// Barva zdí podle prostředí: les zeleně, první podzemí hnědě (hlína),
/// druhé podzemí šedomodře (kámen).
fn wall_color(level: u8) -> Color {
    match level {
        0 => rgb_or((0x3C, 0xB0, 0x3C), Color::Green),
        1 => rgb_or((0xA0, 0x6A, 0x30), Color::Yellow),
        _ => rgb_or((0x70, 0x80, 0xA8), Color::Blue),
    }
}

/// Rámeček okna: aktivní (kam jdou klávesy) bíle, ostatní šedě.
fn framed(active: bool) -> Block<'static> {
    let color = if active { Color::White } else { rgb_or(OLD_COMMAND, Color::DarkGray) };
    Block::bordered().border_style(Style::new().fg(color))
}

/// Mapa v hlavní ploše: vystředěná na hráči, s rolováním, když se nevejde,
/// a dole legenda značek (zdi, jámy, @).
fn draw_big_map(
    frame: &mut Frame,
    area: Rect,
    (title, active): (&str, bool),
    map: &str,
    level: u8,
    state: &mut BigMap,
) -> Option<Position> {
    let help = Line::from(" klik na ↓↑* přepne patro (pravým nahoru) · kolečko posouvá · Esc zpět ")
        .style(Style::new().add_modifier(Modifier::DIM));
    let block = framed(active).title(title.to_owned()).title_bottom(help);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Legenda obecných značek (ikony věcí Jirka v mapě vysvětlovat nechce).
    let legend_text = map::LEGEND.to_owned();
    let legend_paragraph = Paragraph::new(legend_text.clone())
        .wrap(Wrap { trim: true })
        .style(Style::new().fg(rgb_or(OLD_TEXT, Color::Gray)));
    // Přibližně: zalomení po slovech může přidat řádek, proto rezerva +1.
    let legend_width = usize::from(inner.width.max(1));
    let legend_lines: usize =
        legend_text.lines().map(|line| line.chars().count().div_ceil(legend_width).max(1)).sum::<usize>() + 1;
    let legend_height = u16::try_from(legend_lines).unwrap_or(u16::MAX);
    let [map_area, legend_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(legend_height)]).areas(inner);
    frame.render_widget(legend_paragraph, legend_area);

    let lines: Vec<&str> = map.lines().collect();
    let (width, height) = (usize::from(map_area.width), usize::from(map_area.height));
    let content_width = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
    if state.center {
        // Na hráče, a když na prohlíženém patře není, na střed patra.
        let (row, col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.chars().position(|c| c == '@').map(|col| (row, col)))
            .unwrap_or((lines.len() / 2, content_width / 2));
        state.row = row.saturating_sub(height / 2);
        state.col = col.saturating_sub(width / 2);
        state.center = false;
    }
    state.row = state.row.min(lines.len().saturating_sub(height));
    state.col = state.col.min(content_width.saturating_sub(width));
    // Mapa menší než plocha se vystředí, větší se roluje.
    let top = height.saturating_sub(lines.len()) / 2;
    let left = width.saturating_sub(content_width) / 2;
    // Kde na obrazovce by byl řádek 0 a sloupec 0 mapy (pro klik myší).
    let to_i32 = |v: usize| i32::try_from(v).unwrap_or(i32::MAX);
    state.origin = (
        i32::from(map_area.y) + to_i32(top) - to_i32(state.row),
        i32::from(map_area.x) + to_i32(left) - to_i32(state.col),
    );
    let visible: Vec<Line> = std::iter::repeat_n(Line::default(), top)
        .chain(lines.iter().skip(state.row).take(height - top).map(|line| {
            let line: String = line.chars().skip(state.col).take(width).collect();
            map_line(&line, left, level)
        }))
        .collect();
    frame.render_widget(Paragraph::new(visible), map_area);
    // Kde na obrazovce je @ (pro bublinu s radou), pokud je vidět.
    lines.iter().enumerate().find_map(|(row, line)| {
        let col = line.chars().position(|c| c == '@')?;
        let y = state.origin.0 + to_i32(row);
        let x = state.origin.1 + to_i32(col);
        let position = Position::new(u16::try_from(x).ok()?, u16::try_from(y).ok()?);
        map_area.contains(position).then_some(position)
    })
}

/// Bublina s radou u postavy: nad ní, a když nahoře není místo, pod ní.
/// Velikost a zalomení podle celého textu, vidět je prvních `revealed` znaků.
fn draw_bubble(frame: &mut Frame, area: Rect, player: Position, text: &str, revealed: usize) {
    let full_width = text.chars().count().clamp(1, BUBBLE_WIDTH);
    let mut left = revealed;
    let lines: Vec<String> = word_wrap(text, full_width)
        .into_iter()
        .map(|line| {
            let shown: String = line.chars().take(left).collect();
            // Mezera mezi řádky se počítá jako jeden napsaný znak.
            left = left.saturating_sub(line.chars().count() + 1);
            shown
        })
        .collect();
    let width = u16::try_from(full_width + 4).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(lines.len().max(1) + 2).unwrap_or(u16::MAX);
    let x = player.x.saturating_sub(width / 2).clamp(area.x, area.right().saturating_sub(width));
    let above = player.y > area.y + height;
    let (y, tail_y, tail) = if above {
        (player.y - height - 1, player.y - 1, '▼')
    } else {
        (player.y + 2, player.y + 1, '▲')
    };
    let rect = Rect::new(x, y, width, height).intersection(area);
    let style = Style::new().fg(ink(Ink::White, true));
    frame.render_widget(Clear, rect);
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(style);
    let inner = block.inner(rect).inner(ratatui::layout::Margin::new(1, 0));
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines.join("\n")).style(style.add_modifier(Modifier::ITALIC)), inner);
    let tail_rect = Rect::new(player.x, tail_y, 1, 1).intersection(area);
    frame.render_widget(Paragraph::new(tail.to_string()).style(style), tail_rect);
}

/// Zalomí text po slovech na `width` znaků.
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().expect("aspoň jeden řádek");
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_owned());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}

impl LineReader for Tui {
    fn read_line(&mut self, prompt: &str, out: &mut dyn Write) -> io::Result<Option<String>> {
        let mode = if prompt.starts_with(super::COMMAND_PROMPT) { Mode::Command } else { Mode::Answer };
        match self.wait(prompt, mode, out)? {
            Outcome::Line(line) => Ok(Some(line)),
            Outcome::Key(_) | Outcome::Eof => Ok(None),
        }
    }

    fn read_key(&mut self, prompt: &str, out: &mut dyn Write) -> io::Result<Option<char>> {
        let mode = if prompt.is_empty() { Mode::YesNo } else { Mode::AnyKey };
        match self.wait(prompt, mode, out)? {
            Outcome::Key(key) => Ok(Some(key)),
            Outcome::Line(_) | Outcome::Eof => Ok(None),
        }
    }

    fn set_scene(&mut self, scene: Scene) {
        // Bublina jen při prvním vstupu do místnosti (o tom rozhoduje konzole);
        // při další akci zmizí.
        self.bubble = scene.bubble.clone().map(|text| Bubble { text, started: Instant::now() });
        self.scene = scene;
        // Po pohybu se mapa vrátí na patro hráče a vystředí se na něj.
        self.big_map.viewed = None;
        self.big_map.center = true;
    }

    fn finish(&mut self) -> io::Result<()> {
        // Nechat dopsat poslední text a chvíli ho ukázat (nebo do stisku klávesy).
        let deadline = Instant::now() + Duration::from_secs(3);
        self.last_tick = Instant::now();
        while Instant::now() < deadline {
            self.tick();
            self.draw("", Mode::AnyKey)?;
            if event::poll(Duration::from_millis(16))?
                && matches!(event::read()?, Event::Key(_) | Event::Mouse(MouseEvent { kind: MouseEventKind::Down(_), .. }))
            {
                break;
            }
        }
        Ok(())
    }
}

fn is_ctrl(key: KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

fn is_typing(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
}

fn draw_text(frame: &mut Frame, area: Rect, screen: &Screen, scroll: &mut usize, active: bool) {
    let block = framed(active).title(" POKLAD 2 ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = screen.lines(usize::from(inner.width.max(1)));
    let height = usize::from(inner.height);
    let max_scroll = lines.len().saturating_sub(height);
    *scroll = (*scroll).min(max_scroll);
    let end = lines.len() - *scroll;
    let start = end.saturating_sub(height);
    frame.render_widget(Paragraph::new(lines[start..end].to_vec()), inner);
}

/// Co se kreslí do řádku pro psaní.
struct InputLine<'a> {
    prompt: &'a str,
    mode: Mode,
    text: &'a str,
    cursor: usize,
    status: &'a str,
    typing: bool,
}

fn draw_input(frame: &mut Frame, area: Rect, line: &InputLine) {
    let InputLine { prompt, mode, text: input, cursor, status, typing } = *line;
    let help = match mode {
        Mode::Command | Mode::Answer if !status.is_empty() => format!(" {status} "),
        Mode::Command if !typing => {
            " šipky/WSAD pohyb · R/F nahoru/dolů · Tab nebo klik sem = psaní · Shift+PgUp/PgDn text ".to_owned()
        }
        Mode::Command | Mode::Answer => " Enter odešle · Esc nebo Tab (prázdný řádek) zpět k mapě · ↑↓ historie · Tab doplní ".to_owned(),
        Mode::YesNo => " stiskni A nebo N ".to_owned(),
        Mode::AnyKey => String::new(),
    };
    let block = framed(typing).title_bottom(Line::from(help).style(Style::new().add_modifier(Modifier::DIM)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = match mode {
        Mode::Command | Mode::Answer => input,
        Mode::YesNo | Mode::AnyKey => "",
    };
    // Neaktivní řádek je tlumený, aby bylo vidět, že klávesy teď hýbou postavou.
    let prompt_style = if typing || mode != Mode::Command {
        Style::new().fg(ink(Ink::Magenta, true))
    } else {
        Style::new().fg(rgb_or(OLD_COMMAND, Color::DarkGray))
    };
    let line = Line::from(vec![Span::styled(prompt.to_owned(), prompt_style), Span::raw(text.to_owned())]);
    frame.render_widget(Paragraph::new(line), inner);
    if typing {
        let column = prompt.chars().count() + input.chars().take(cursor).count();
        let x = inner.x.saturating_add(u16::try_from(column).unwrap_or(u16::MAX)).min(inner.right().saturating_sub(1));
        frame.set_cursor_position(Position::new(x, inner.y));
    }
}

/// Kolik řádků zabere text zalomený po slovech na `width` znaků.
fn wrapped_height(text: &str, width: usize) -> u16 {
    let width = width.max(1);
    let lines: usize = text
        .lines()
        .map(|line| {
            let mut rows = 1;
            let mut column = 0;
            for word in line.split_whitespace() {
                let len = word.chars().count();
                if column > 0 && column + 1 + len > width {
                    rows += 1;
                    column = 0;
                }
                column += if column == 0 { len } else { len + 1 };
                // Slovo delší než řádek se láme natvrdo.
                rows += column.saturating_sub(1) / width;
                column = (column.saturating_sub(1) % width) + 1;
            }
            rows
        })
        .sum();
    u16::try_from(lines.max(1)).unwrap_or(u16::MAX)
}

/// Kreslení tlačítek a zapamatování, kde jsou, pro kliknutí myší.
struct Buttons<'a, 'f> {
    frame: &'a mut Frame<'f>,
    list: &'a mut Vec<(Rect, Button)>,
    hover: Option<Position>,
}

impl Buttons<'_, '_> {
    /// Postranní panel. S `text_slot` v něm zůstane místo na text hry (po
    /// úvodu, kdy hlavní plochu zabírá mapa); vrací, kde to místo je.
    fn panel(&mut self, area: Rect, mode: Mode, scene: &Scene, text_slot: bool) -> Option<Rect> {
        let here_height = u16::try_from(scene.here.len().max(1)).unwrap_or(u16::MAX) + 2;
        let carried_height = u16::try_from(scene.carried.len().max(1)).unwrap_or(u16::MAX) + 2;
        // Popis místa a rada zalomené na šířku panelu (bez rámečku).
        let inner_width = usize::from(area.width.saturating_sub(2));
        let in_game = scene.turns > 0;
        // Blok "Kde jsi": popis místa a pod ním rada (co by řekl POMOC).
        let status_height = if in_game {
            wrapped_height(&scene.location, inner_width) + wrapped_height(&scene.hint, inner_width) + 2
        } else {
            0
        };
        // Shora text hry, pod ním kde jsem, co vidím, tlačítka a co nesu.
        let [text, status, here, directions, commands, carried] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(status_height),
            Constraint::Length(here_height),
            Constraint::Length(5),
            // Příkazy: dva řádky tlačítek a rámeček.
            Constraint::Length(4),
            Constraint::Length(carried_height),
        ])
        .areas(area);
        if scene.turns > 0 {
            // Hodiny a poklady: totéž, co vypíše INVENTURA, jen trvale a bez
            // spotřebované hodiny.
            let progress = format!(" {} hodin · poklady {}/5 ", scene.turns, scene.treasures);
            let block = framed(false)
                .title(" Kde jsi ")
                .title_bottom(Line::from(progress).style(Style::new().fg(ink(Ink::Yellow, false))));
            let inner = block.inner(status);
            self.frame.render_widget(block, status);
            let [location_area, hint_area] = Layout::vertical([
                Constraint::Length(wrapped_height(&scene.location, inner_width)),
                Constraint::Min(0),
            ])
            .areas(inner);
            let location = Paragraph::new(scene.location.clone())
                .wrap(Wrap { trim: true })
                .style(Style::new().add_modifier(Modifier::BOLD));
            self.frame.render_widget(location, location_area);
            // Rada pro místnost: co by řekl příkaz POMOC, trvale (Jirkovo přání).
            let advice = Paragraph::new(scene.hint.clone())
                .wrap(Wrap { trim: true })
                .style(Style::new().fg(ink(Ink::Cyan, false)).add_modifier(Modifier::ITALIC));
            self.frame.render_widget(advice, hint_area);
        }
        match mode {
            Mode::Command => {
                self.directions(directions, scene);
                self.commands(commands);
            }
            Mode::YesNo => {
                let inner = self.block(directions, " Odpověď ");
                self.button(inner.x + 4, inner.y + 1, "  A  ", Button::Key('a'));
                self.button(inner.x + 18, inner.y + 1, "  N  ", Button::Key('n'));
            }
            Mode::Answer | Mode::AnyKey => {}
        }
        if mode != Mode::YesNo && mode != Mode::AnyKey {
            self.items(here, " Vidíš ", scene, Source::Here);
            self.items(carried, " Neseš ", scene, Source::Carried);
        }
        text_slot.then_some(text)
    }

    fn block(&mut self, area: Rect, title: &str) -> Rect {
        let block = framed(false).title(title.to_owned());
        let inner = block.inner(area);
        self.frame.render_widget(block, area);
        inner
    }

    /// Neaktivní tlačítko: jen šedý nápis, na kliknutí nereaguje.
    fn inactive(&mut self, x: u16, y: u16, label: &str) {
        let text = format!("[{label}]");
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        let rect = Rect::new(x, y, width, 1).intersection(self.frame.area());
        let style = Style::new().fg(rgb_or(OLD_COMMAND, Color::DarkGray));
        self.frame.render_widget(Paragraph::new(text).style(style), rect);
    }

    fn button(&mut self, x: u16, y: u16, label: &str, button: Button) -> u16 {
        let text = format!("[{label}]");
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        let area = self.frame.area();
        let rect = Rect::new(x, y, width, 1).intersection(area);
        let hovered = self.hover.is_some_and(|p| rect.contains(p));
        let style = if hovered {
            Style::new().fg(Color::Black).bg(ink(Ink::Cyan, true))
        } else {
            Style::new().fg(ink(Ink::Cyan, false))
        };
        self.frame.render_widget(Paragraph::new(text).style(style), rect);
        self.list.push((rect, button));
        width
    }

    /// Směry: kudy vede cesta, tlačítko funguje; kde je zeď, je šedé a neaktivní.
    fn directions(&mut self, area: Rect, scene: &Scene) {
        let inner = self.block(area, " Kudy ");
        let (x, y) = (inner.x, inner.y);
        let exit = |dir| scene.exits.contains(&dir);
        let buttons = [
            (exit(Dir::North), x + 6, y, "Sever", "sever"),
            (scene.up, x + 21, y, "Nahoru", "nahoru"),
            (exit(Dir::West), x, y + 1, "Západ", "západ"),
            (exit(Dir::East), x + 11, y + 1, "Východ", "východ"),
            (exit(Dir::South), x + 7, y + 2, "Jih", "jih"),
            (scene.down, x + 22, y + 2, "Dolů", "dolů"),
        ];
        for (open, x, y, label, command) in buttons {
            if open {
                self.button(x, y, label, Button::Run(command));
            } else {
                self.inactive(x, y, label);
            }
        }
    }

    fn commands(&mut self, area: Rect) {
        // INVENTURA tlačítko nemá: stav je trvale v panelu (jde ale napsat).
        // MAPA také ne: mapa je po úvodu vidět pořád. POMOC a NÁVOD taky ne:
        // rada je trvale v bloku Kde jsi. Všechny ty příkazy jdou napsat.
        const COMMANDS: [(&str, Button); 7] = [
            ("Vezmi…", Button::Start("vezmi")),
            ("Polož…", Button::Start("polož")),
            ("Zastřel", Button::Run("zastřel")),
            ("Rozsviť", Button::Run("rozsviť")),
            ("Save", Button::Run("save")),
            ("Load", Button::Run("load")),
            ("Konec", Button::Run("konec")),
        ];
        let inner = self.block(area, " Příkazy ");
        let (mut x, mut y) = (inner.x, inner.y);
        for (label, button) in COMMANDS {
            let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX) + 2;
            if x + width > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= inner.bottom() {
                break;
            }
            x += self.button(x, y, label, button) + 1;
        }
    }

    fn items(&mut self, area: Rect, title: &str, scene: &Scene, source: Source) {
        let items = if source == Source::Here { &scene.here } else { &scene.carried };
        let inner = self.block(area, title);
        if items.is_empty() {
            let empty = Paragraph::new("nic").style(Style::new().add_modifier(Modifier::DIM));
            self.frame.render_widget(empty, inner);
            return;
        }
        for (row, item) in (inner.y..inner.bottom()).zip(items) {
            let name = scene.name(*item);
            let label = format!("{} {}", map::icon(*item), name);
            self.button(inner.x, row, &label, Button::Item(name.to_owned(), source));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styles(screen: &Screen) -> Vec<(String, Style)> {
        screen
            .lines(80)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| (span.content.into_owned(), span.style))
            .collect()
    }

    #[test]
    fn colors_from_ansi_survive_split_writes() {
        let mut screen = Screen::default();
        screen.feed(b"\x1b[3");
        screen.feed("6mPříkaz\x1b[0m ok".as_bytes());
        let text: String = screen.cells.iter().map(|&(c, _)| c).collect();
        assert_eq!(text, "Příkaz ok");
        assert_eq!(screen.cells[0].1.fg, Some(ink(Ink::Cyan, false)));
        screen.feed(b"\x1b[38;2;255;255;0mRGB");
        assert_eq!(screen.cells.last().map(|cell| cell.1.fg), Some(Some(Color::Rgb(255, 255, 0))));
        assert_eq!(screen.cells[7].1.fg, None);
    }

    #[test]
    fn commands_and_answers_have_own_colors() {
        let mut screen = Screen::default();
        screen.feed(b"Prikaz ?: sever\n");
        screen.mark_input(0);
        screen.feed(b"Jsi v lese.\n");
        screen.start_latest();
        let start = screen.cells.len();
        screen.feed(b"Prikaz ?: jih\n");
        screen.mark_input(start);
        screen.feed(b"Tam se neda jit");
        screen.reveal_all();
        let spans = styles(&screen);
        let old_command = Some(rgb_or(OLD_COMMAND, Color::DarkGray));
        let old_text = Some(rgb_or(OLD_TEXT, Color::Gray));
        let latest_command = Some(rgb_or(LATEST_COMMAND, Color::Indexed(130)));
        assert_eq!((spans[0].0.as_str(), spans[0].1.fg), ("Prikaz ?: sever", old_command));
        assert_eq!((spans[1].0.as_str(), spans[1].1.fg), ("Jsi v lese.", old_text));
        assert_eq!((spans[2].0.as_str(), spans[2].1.fg), ("Prikaz ?: jih", latest_command));
        assert_eq!((spans[3].0.as_str(), spans[3].1.fg), ("Tam se neda jit", Some(ink(Ink::Yellow, true))));
    }

    #[test]
    fn bubble_text_wraps_and_types_out() {
        assert_eq!(word_wrap("Já bych si dal pozor...", 30), ["Já bych si dal pozor..."]);
        assert_eq!(word_wrap("Drak sežere, co mu podstrčíš...", 14), ["Drak sežere,", "co mu", "podstrčíš..."]);
        let mut bubble = Bubble { text: "Počínáš".to_owned(), started: Instant::now() };
        assert!(bubble.writing());
        bubble.finish();
        assert!(!bubble.writing());
        assert_eq!(bubble.revealed(), 7);
    }

    #[test]
    fn wrapped_height_counts_word_wrapped_rows() {
        assert_eq!(wrapped_height("Počínáš si dobře...", 32), 1);
        // "Straky kradou drahé kovy, co" | "když má tahle něco v" | "žaludku...?"
        assert_eq!(wrapped_height("Straky kradou drahé kovy, co když má tahle něco v žaludku...?", 28), 3);
        assert_eq!(wrapped_height("abcdefghij", 4), 3);
        assert_eq!(wrapped_height("a\nb", 10), 2);
    }

    #[test]
    fn wraps_on_word_boundaries() {
        let mut screen = Screen::default();
        screen.feed(b"jedna dva tri ctyri\nnejdelsislovo");
        screen.reveal_all();
        let text: Vec<String> = screen.lines(9).iter().map(ToString::to_string).collect();
        assert_eq!(text, ["jedna dva", "tri ctyri", "nejdelsis", "lovo"]);
    }
}
