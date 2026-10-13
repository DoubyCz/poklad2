//! Terminálové rozhraní: výpisy, příkazy navíc (MAPA, NÁVOD), ukládání pozic.
//!
//! Herní texty jsou doslova z originálu. Co je v rozhraní nové, je
//! vypsané stylem `Style::Extra`, aby se to od původní hry dalo odlišit.

mod input;
mod map;
mod style;
mod tui;

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::rc::Rc;

use crate::data::{self, Dir, Item};
use crate::game::{self, Action, Game, Response, Turn};
use crate::parser::{self, Verb};
use input::{LineReader, PlainReader, Scene};
use map::{Map, Scale};
use style::{Palette, Style};
use tui::{Screen, ScreenWriter, Tui};

/// Výzva k hernímu příkazu; celoobrazovkové rozhraní podle ní pozná, že má
/// nabízet tlačítka příkazů.
const COMMAND_PROMPT: &str = "Příkaz ?:";

const TITLE: &str = "
    ███ ███ █  █ █   ███ ██
    █ █ █ █ █ █  █   █ █ █ █
    ███ █ █ ██   █   ███ █ █
    █   █ █ █ █  █   █ █ █ █
    █   ███ █  █ ███ █ █ ██

          (VERZE 2.1)

  František Fuka   FUXOFT 1985

      speech by VOICESOFT

         Počkej prosím";

/// Řádky instrukcí doslova z originálu (ř. 9080-9083 a 9910). Fuka je psal
/// na šířku obrazovky Spectra, mezery a dělení slov počítají s 32 sloupci;
/// před výpisem je skládá `reflow`.
const INSTRUCTIONS: [&str; 9] = [
    "           INSTRUKCE",
    "Seznam příkazů :",
    "S         J         Z",
    "V         N         D",
    "VEZMI     POLOZ     ZASTREL",
    "INVENTURA KONEC     ROZSVIT     POMOC     SAVE      LOAD",
    "Tvým úkolem je najít 5 pokladů.",
    "Při zadávání příkazů stačí počí-tači pouze tři první písmena.   Je-li příkaz dvouslovný (např.  VEZMI MAPU), zadej pouze VEZ a  stiskni ENTER. Počítač se zeptá,co chceš vzít.",
    "Chceš-li použít nějaký předmět  ke zničení nebo zahnání něčeho  nebo někoho, vejdi do místnosti,kde se nepřítel vyskytuje       a předmět zde polož. (Toto ne-  platí pro střelné zbraně, u nichpoužij příkaz ZASTREL)",
];

/// Šířka obrazovky ZX Spectra ve znacích.
const SCREEN_WIDTH: usize = 32;

/// Složí herní text psaný pro 32 sloupců Spectra do souvislých vět, aby ho
/// šlo zalomit na libovolnou šířku. Řetězce v kódu zůstávají doslova, mění
/// se jen zobrazení:
/// - konec řádku Spectra funguje jako mezera ("u nich|použij"),
/// - pomlčka na konci plného řádku je dělené slovo ("počí-|tači" = počítači),
/// - zarovnávací mezery se slijí do jedné.
///
/// Řádky, které se na Spectrum vešly celé, zůstávají beze změny (zarovnání
/// do sloupců, odsazení).
fn reflow(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() <= SCREEN_WIDTH {
                return line.to_owned();
            }
            let mut joined = String::new();
            let mut split_word = false;
            for chunk in chars.chunks(SCREEN_WIDTH) {
                let piece: String = chunk.iter().collect();
                let piece = piece.trim_end();
                if !joined.is_empty() && !split_word {
                    joined.push(' ');
                }
                joined.push_str(piece);
                split_word = chunk.len() == SCREEN_WIDTH
                    && piece.ends_with('-')
                    && piece.chars().rev().nth(1).is_some_and(char::is_alphabetic);
                if split_word {
                    joined.pop();
                }
            }
            joined.split(' ').filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const NEWS: &str = "
Novinky tohoto přepisu:
- po úvodu je v hlavním okně mapa patra, kde jsi, se zdmi a věcmi; text hry je vpravo
- klik na jámu (↓ ↑) nebo přenesení (*) v mapě ukáže patro, kam vede (pravým nahoru), Esc vrátí zpět
- pohyb klávesami: šipky nebo W/S/A/D, nahoru a dolů R/F (nebo PageUp/PageDown)
- psát příkazy jde po kliknutí do řádku dole (nebo Tab, Enter); Esc nebo Tab v prázdném řádku vrátí k pohybu
- příkazy jde i naklikat myší (Vezmi… a klik na předmět), klik na předmět ho vezme nebo položí
- VEZMI DIAMANT jde napsat na jeden řádek, směry i celými slovy (SEVER), na diakritice nezáleží
- v řádku šipky listují historií příkazů a Tab doplňuje
- NÁVOD (nebo ?) zopakuje tuto obrazovku a nezabere herní hodinu
- klávesa nebo klik dopíše text hned, Ctrl+D hru ukončí";

const MAX_SAVE_NAME: usize = 10;

/// Jak skončila jedna hra.
#[derive(PartialEq, Eq)]
enum Ending {
    /// Smrt, výhra nebo KONEC: nabídnout novou hru.
    Over,
    /// Došel vstup (Ctrl+D).
    Eof,
}

struct Console<I, W> {
    input: I,
    output: W,
    palette: Palette,
}

/// Spustí hru na standardním vstupu a výstupu.
pub fn run_terminal() -> io::Result<()> {
    let palette = Palette::detect();
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        let screen = Rc::new(RefCell::new(Screen::default()));
        run(Tui::new(Rc::clone(&screen))?, ScreenWriter(screen), palette)
    } else {
        run(PlainReader::new(io::stdin().lock(), palette), io::stdout(), palette)
    }
}

fn run(input: impl LineReader, output: impl Write, palette: Palette) -> io::Result<()> {
    let mut console = Console { input, output, palette };
    let result = match console.run() {
        // Ctrl+C: originál na přerušení programu (BREAK) reagoval touhle
        // hláškou a resetem počítače, přepis se s ní ukončí.
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            console.say("\nCo to má znamenat ?!", Style::Death)
        }
        result => result,
    };
    console.input.finish()?;
    result
}

impl<I: LineReader, W: Write> Console<I, W> {
    fn run(&mut self) -> io::Result<()> {
        self.say(TITLE, Style::Title)?;
        self.show_instructions()?;
        self.say("", Style::Message)?;
        if self.input.read_key("   Stiskni libovolné tlačítko   ", &mut self.output)?.is_none() {
            return Ok(());
        }
        while self.play()? == Ending::Over {
            if !self.yes_no("Chceš to zkusit znovu ? (A/N)", Style::Death)? {
                break;
            }
        }
        Ok(())
    }

    fn say(&mut self, text: &str, style: Style) -> io::Result<()> {
        writeln!(self.output, "{}", self.palette.paint(text, style))
    }

    /// Herní text z originálu, složený z řádků Spectra do souvislých vět.
    fn tell(&mut self, text: &str, style: Style) -> io::Result<()> {
        self.say(&reflow(text), style)
    }

    fn ask(&mut self, prompt: &str) -> io::Result<Option<String>> {
        self.input.read_line(&format!("{prompt} "), &mut self.output)
    }

    /// Otázka A/N jako v originálu: čeká na stisk A nebo N, jiné klávesy ignoruje.
    fn yes_no(&mut self, question: &str, style: Style) -> io::Result<bool> {
        self.say(&format!("\n{question}"), style)?;
        loop {
            match self.input.read_key("", &mut self.output)? {
                Some('a' | 'A') => return Ok(true),
                None | Some('n' | 'N') => return Ok(false),
                Some(_) => {}
            }
        }
    }

    fn show_instructions(&mut self) -> io::Result<()> {
        self.say("", Style::Message)?;
        for line in INSTRUCTIONS {
            self.tell(line, Style::Message)?;
        }
        self.say(NEWS, Style::Extra)
    }

    fn show_turn(&mut self, turn: &Turn, game: &Game) -> io::Result<()> {
        self.tell(&format!("\n{}", turn.location), Style::Location)?;
        if !turn.alive {
            return self.tell(&format!("\n\n{}", game::DEATH), Style::Death);
        }
        if turn.items.is_empty() {
            return self.tell(game::NOTHING_SEEN, Style::Location);
        }
        self.tell(game::SEEN, Style::Location)?;
        for item in &turn.items {
            self.tell(game.item_name(*item), Style::Item)?;
        }
        Ok(())
    }

    fn play(&mut self) -> io::Result<Ending> {
        let mut game = Game::new();
        let mut map = Map::new(game.room());
        // Místnosti, kde už se ukázala bublina (jen při prvním vstupu).
        let mut greeted = HashSet::new();
        loop {
            let turn = game.begin_turn();
            self.show_turn(&turn, &game)?;
            if !turn.alive {
                return Ok(Ending::Over);
            }
            let room = game.room();
            let level = map::level_of(room);
            // Zdi kolem místa, kam hráč vstoupil, jsou hned vidět (Jirkovo přání):
            // na mapě i v tlačítkách, která nabízí jen směry bez zdi.
            let (exits, walls): (Vec<Dir>, Vec<Dir>) = [Dir::North, Dir::East, Dir::South, Dir::West]
                .into_iter()
                .partition(|&dir| data::neighbor(room, dir).is_some());
            for dir in walls {
                map.record_wall(room, dir);
            }
            self.input.set_scene(Scene {
                here: game.items_here(),
                carried: game.items_carried(),
                location: reflow(&turn.location),
                turns: game.turns(),
                treasures: game.treasures(),
                exits,
                up: data::shaft_up(room).is_some(),
                down: data::shaft_down(room).is_some(),
                // Pod @ se značka jámy nevejde, proto je v titulku.
                map_title: match map.portal_symbol(game.room()) {
                    Some(symbol) => format!(" Mapa: {} · tady {symbol} ", map::level_name(level)),
                    None => format!(" Mapa: {} ", map::level_name(level)),
                },
                map_frames: {
                    let contents = map_contents(&game, &map);
                    let frames = map.frame_count(&contents);
                    map.visited_levels()
                        .into_iter()
                        .map(|level| {
                            let render = |frame| map.level_lines(level, room, Scale::Large, &contents, frame).join("\n");
                            (level, (0..frames).map(render).collect())
                        })
                        .collect()
                },
                player_level: level,
                portals: map
                    .visited()
                    .map(|place| (place, map.connected_levels(place)))
                    .filter(|(_, levels)| !levels.is_empty())
                    .collect(),
                door_unlocked: game.item_name(Item::Door) == game::UNLOCKED_DOOR,
                hint: reflow(game.hint()),
                bubble: greeted.insert(room).then(|| bubble_text(&game)).flatten(),
            });
            let line = loop {
                let Some(line) = self.ask(COMMAND_PROMPT)? else {
                    return Ok(Ending::Eof);
                };
                if !self.extra_command(&line, &game, &map)? {
                    break line;
                }
            };
            if let Some(ending) = self.command(&mut game, &mut map, &line)? {
                return Ok(ending);
            }
        }
    }

    /// Příkazy rozhraní, které nezaberou herní hodinu. `true` = vyřízeno.
    fn extra_command(&mut self, line: &str, game: &Game, map: &Map) -> io::Result<bool> {
        let word = parser::normalize(line);
        let word = word.split_whitespace().next().unwrap_or("");
        if word.is_empty() {
            return Ok(true);
        }
        if word.starts_with("MAP") {
            let contents = map_contents(game, map);
            let text = map.render(game.room(), Scale::Small, &contents, 0);
            let legend = map_legend(&contents).join("  ");
            self.say(&format!("\n{text}\n{}\n{legend}", map::LEGEND), Style::Extra)?;
        } else if word.starts_with("NAV") || word == "?" {
            self.show_instructions()?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// Provede jeden herní příkaz. `Some` = hra skončila.
    fn command(&mut self, game: &mut Game, map: &mut Map, line: &str) -> io::Result<Option<Ending>> {
        let Some(command) = parser::parse(line) else {
            self.say(game::UNKNOWN_COMMAND, Style::Message)?;
            return Ok(None);
        };
        let action = match command.verb {
            Verb::Go(dir) => Action::Go(dir),
            Verb::Up => Action::Up,
            Verb::Down => Action::Down,
            Verb::Shoot => Action::Shoot,
            Verb::Inventory => Action::Inventory,
            Verb::Light => Action::Light,
            Verb::Balalajka => Action::Balalajka,
            Verb::Allah => Action::Allah,
            Verb::Help => Action::Help,
            Verb::Take | Verb::Drop => {
                let object = match command.object {
                    Some(object) => object,
                    None => match self.ask("Co ?:")? {
                        Some(object) => object,
                        None => return Ok(Some(Ending::Eof)),
                    },
                };
                let Some(item) = parser::find_item(&object) else {
                    self.say(game::UNKNOWN_ITEM, Style::Message)?;
                    return Ok(None);
                };
                if command.verb == Verb::Take { Action::Take(item) } else { Action::Drop(item) }
            }
            Verb::Quit => {
                let quit = self.yes_no("Chceš opravdu skončit ?(A/N)", Style::Message)?;
                return Ok(quit.then_some(Ending::Over));
            }
            Verb::Save => {
                self.save(game, command.object)?;
                return Ok(None);
            }
            Verb::Load => {
                if self.load(game, command.object)? {
                    *map = Map::new(game.room());
                }
                return Ok(None);
            }
        };
        let from = game.room();
        let response = game.execute(action);
        map.record(from, game.room());
        // "Tam se nedá jít" je zeď (na rozdíl od "Prozatím tam nemůžeš").
        if let Action::Go(dir) = action
            && matches!(&response, Response::Message(text) if text == game::CANT_GO)
        {
            map.record_wall(from, dir);
        }
        match response {
            Response::Silent => {}
            Response::Message(text) => {
                let style = if action == Action::Help { Style::Hint } else { Style::Message };
                self.tell(&text, style)?;
            }
            Response::Won => {
                let text = format!(
                    "\n\nDokázal jsi to za {} hodin !\n  GRATULUJI!\nheslo:FS3298685",
                    game.turns(),
                );
                self.tell(&text, Style::Win)?;
                return Ok(Some(Ending::Over));
            }
        }
        Ok(None)
    }

    /// Zjistí jméno pozice (z příkazu, nebo se zeptá) a vrátí cestu k souboru.
    fn save_path(&mut self, name: Option<String>, prompt: &str) -> io::Result<Option<PathBuf>> {
        let name = match name {
            Some(name) => name,
            None => match self.ask(prompt)? {
                Some(name) => name,
                None => return Ok(None),
            },
        };
        if name.is_empty() {
            return Ok(None);
        }
        let name = parser::normalize(&name).to_lowercase();
        if name.len() > MAX_SAVE_NAME || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            self.say("Maximálně 10 písmen !!", Style::Hint)?;
            return Ok(None);
        }
        let Some(dir) = save_dir() else {
            self.say("Nevím, kam pozice ukládat (chybí HOME).", Style::Extra)?;
            return Ok(None);
        };
        Ok(Some(dir.join(format!("{name}.sav"))))
    }

    fn save(&mut self, game: &Game, name: Option<String>) -> io::Result<()> {
        // Původní otázka zůstává, i když se ukládá do souboru.
        if !self.yes_no("Nahrát pozici na magnetofon ?", Style::Message)? {
            return Ok(());
        }
        let Some(path) = self.save_path(name, "Pod jakým jménem:")? else {
            return Ok(());
        };
        let result = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, game.to_save()));
        match result {
            Ok(()) => self.say(&format!("Uloženo do {}", path.display()), Style::Extra),
            Err(err) => self.say(&format!("Pozici se nepodařilo uložit: {err}"), Style::Extra),
        }
    }

    /// Nahraje pozici. `true` = hra se změnila.
    fn load(&mut self, game: &mut Game, name: Option<String>) -> io::Result<bool> {
        if !self.yes_no("Nahrát pozici z magnetofonu ?", Style::Message)? {
            return Ok(false);
        }
        if name.is_none() {
            let saves = saved_positions();
            if saves.is_empty() {
                self.say("Zatím nemáš uloženou žádnou pozici.", Style::Extra)?;
                return Ok(false);
            }
            self.say(&format!("Uložené pozice: {}", saves.join(", ")), Style::Extra)?;
        }
        let Some(path) = self.save_path(name, "Název pozice:")? else {
            return Ok(false);
        };
        if let Some(loaded) = fs::read_to_string(&path).ok().and_then(|text| Game::from_save(&text)) {
            *game = loaded;
            return Ok(true);
        }
        self.say("Takovou pozici nemám.", Style::Extra)?;
        Ok(false)
    }
}

/// Bublina při prvním vstupu do místnosti: co tu leží ("Vidím studnu.")
/// a rada, pokud něco říká. `None`, když není co říct.
fn bubble_text(game: &Game) -> Option<String> {
    let names: Vec<&str> = game.items_here().into_iter().map(|item| game.item_name(item)).collect();
    let seen = match names.as_slice() {
        [] => None,
        [one] => Some(format!("Vidím {one}.")),
        [rest @ .., last] => Some(format!("Vidím {} a {last}.", rest.join(", "))),
    };
    let hint = game.hint();
    let hint = (hint != data::DEFAULT_HINT).then(|| reflow(hint));
    match (seen, hint) {
        (None, None) => None,
        (seen, hint) => Some(seen.into_iter().chain(hint).collect::<Vec<_>>().join(" ")),
    }
}

/// Věci v místnostech, kde hráč byl, kromě té, kde stojí (tu ukazují panely).
/// Věci se mění jen akcemi hráče přímo v místnosti, takže mapa ukazuje to,
/// co tam hráč naposledy viděl.
fn map_contents(game: &Game, map: &Map) -> map::Contents {
    map.visited()
        .filter(|&room| room != game.room())
        .map(|room| (room, game.items_at(room)))
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

/// Legenda ikon věcí na mapě, v pořadí seznamu předmětů.
fn map_legend(contents: &map::Contents) -> Vec<String> {
    let mut items: Vec<_> = contents.values().flatten().copied().collect();
    items.sort_by_key(|item| item.index());
    items.dedup();
    items.into_iter().map(|item| format!("{} {}", map::icon(item), map::label(item))).collect()
}

/// `$XDG_DATA_HOME/poklad2`, jinak `~/.local/share/poklad2`; na Windows
/// (bez `HOME`) `%APPDATA%\poklad2`.
fn save_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    Some(base.join("poklad2"))
}

fn saved_positions() -> Vec<String> {
    let Some(entries) = save_dir().and_then(|dir| fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "sav").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_says_what_is_here_and_the_hint() {
        let mut game = Game::new();
        assert_eq!(bubble_text(&game), None);
        game.execute(Action::Go(Dir::East));
        assert_eq!(bubble_text(&game).as_deref(), Some("Vidím zhasnutou svítilnu."));
    }

    #[test]
    fn reflow_joins_spectrum_lines_back_into_sentences() {
        assert_eq!(
            reflow(INSTRUCTIONS[7]),
            "Při zadávání příkazů stačí počítači pouze tři první písmena. Je-li příkaz \
             dvouslovný (např. VEZMI MAPU), zadej pouze VEZ a stiskni ENTER. Počítač se \
             zeptá, co chceš vzít."
        );
        assert_eq!(
            reflow(INSTRUCTIONS[8]),
            "Chceš-li použít nějaký předmět ke zničení nebo zahnání něčeho nebo někoho, \
             vejdi do místnosti, kde se nepřítel vyskytuje a předmět zde polož. (Toto \
             neplatí pro střelné zbraně, u nich použij příkaz ZASTREL)"
        );
        // Krátké řádky (zarovnání do sloupců, odsazení) zůstávají.
        assert_eq!(reflow("S         J         Z"), "S         J         Z");
        assert_eq!(reflow(game::DEATH), "Máš vybité baterie ve svítilně nebo zhasnuto.\n       Jsi MRTEV !");
    }

    /// Kompletní průchod hrou: všechny hádanky a pět pokladů.
    const WALKTHROUGH: &str = include_str!("../../tests/pruchod.txt");

    fn play(input: &str) -> String {
        let mut output = Vec::new();
        run(PlainReader::new(input.as_bytes(), Palette::plain()), &mut output, Palette::plain())
            .unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn walkthrough_wins() {
        let output = play(&format!("\n{WALKTHROUGH}n\n"));
        assert!(output.contains("Dokázal jsi to za 108 hodin !\n  GRATULUJI!"), "{output}");
        assert!(output.contains("heslo:FS3298685"), "{output}");
        assert!(!output.contains("MRTEV"), "{output}");
        assert!(!output.contains(game::NOT_YET), "{output}");
        assert!(!output.contains(game::UNKNOWN_ITEM), "{output}");
    }

    #[test]
    fn errors_are_reported() {
        let output = play("\ns\npoloz\nnic\nzzz\nkonec\na\nn\n");
        assert!(output.contains(game::CANT_GO), "{output}");
        assert!(output.contains(game::UNKNOWN_ITEM), "{output}");
        assert!(output.contains(game::UNKNOWN_COMMAND), "{output}");
        assert!(output.contains("Chceš opravdu skončit ?(A/N)"), "{output}");
    }

    #[test]
    fn extra_commands_do_not_take_a_turn() {
        let output = play("\nmapa\nnavod\n\ninventura\nkonec\na\nn\n");
        assert!(output.contains("[@]"), "{output}");
        assert!(output.contains("Bloudíš už 1 hodin."), "{output}");
    }

    #[test]
    fn dying_in_the_dark() {
        // Průchod až k jámě v místnosti 2, ale dolů bez rozsvícení svítilny.
        let until_pit = WALKTHROUGH.split("rozsvit").next().unwrap();
        let output = play(&format!("\n{until_pit}d\nn\n"));
        assert!(output.contains("Máš vybité baterie ve svítilně nebo zhasnuto.\n       Jsi MRTEV !"));
        assert!(output.contains("Chceš to zkusit znovu ? (A/N)"), "{output}");
    }
}
