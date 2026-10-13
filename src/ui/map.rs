//! Mapa prozkoumaných míst: místnosti, kde hráč byl, prošlé průchody a zdi
//! kolem navštívených místností (ty se kreslí hned při vstupu). O místech,
//! kde hráč nebyl, nic neprozradí.

use std::collections::{HashMap, HashSet};

use crate::data::{Dir, Item, Room};

/// Zeď, do které hráč narazil (šedě v rozhraní). Průchody jsou `─` a `│`.
pub const WALL_HORIZONTAL: char = '═';
pub const WALL_VERTICAL: char = '║';

const LEVELS: [&str; 3] = ["les", "1. podzemí", "2. podzemí"];
const COLUMNS: Room = 4;
const ROWS: Room = 5;
const LEVEL_SIZE: Room = COLUMNS * ROWS;
const GAP: &str = "  ";

/// Velikost kreslení: malá (`[@]`, spojky o 1 znaku) pro minimapu a příkaz
/// MAPA, velká (`[ @ ]`, delší spojky) pro zvětšenou mapu.
#[derive(Clone, Copy)]
pub enum Scale {
    Small,
    Large,
}

impl Scale {
    /// Mezery uvnitř `[ ]`, délka vodorovné spojky, počet řádků svislé spojky.
    const fn parts(self) -> (usize, usize, usize) {
        match self {
            Self::Small => (0, 1, 1),
            Self::Large => (1, 3, 2),
        }
    }

    const fn cell_width(self) -> usize {
        3 + 2 * self.parts().0
    }

    /// Šířka patra: okraj na zdi, 4 místnosti a 3 spojky mezi nimi, okraj.
    const fn level_width(self) -> usize {
        2 + 4 * self.cell_width() + 3 * self.parts().1
    }

    /// Výška patra: okraj, 5 řad místností, mezi nimi spojky, okraj.
    const fn level_height(self) -> usize {
        2 + ROWS as usize + (ROWS as usize - 1) * self.parts().2
    }

    /// Místnost patra, jejíž buňka leží na řádku `y` a sloupci `x` (pro klik).
    pub fn room_at(self, level: Room, y: usize, x: usize) -> Option<Room> {
        (level * LEVEL_SIZE + 1..=(level + 1) * LEVEL_SIZE).find(|&room| {
            let (top, left) = self.cell_origin(room);
            y == top && (left..left + self.cell_width()).contains(&x)
        })
    }

    /// Kde v patře leží levý horní roh buňky místnosti (řádek, sloupec).
    const fn cell_origin(self, room: Room) -> (usize, usize) {
        let index = (room - 1) % LEVEL_SIZE;
        let (row, column) = ((index / COLUMNS) as usize, (index % COLUMNS) as usize);
        let (_, link_len, link_rows) = self.parts();
        (1 + row * (1 + link_rows), 1 + column * (self.cell_width() + link_len))
    }
}

pub const LEGEND: &str = "@ jsi tady   ↓ ↑ ↕ jáma do jiného patra   * přenesení jinam   ═ ║ zeď";

/// Prošlé cesty z místnosti do jiného patra.
#[derive(Clone, Copy, Default)]
struct Portal {
    up: bool,
    down: bool,
    /// Přenesení jinam (kouzelné slovo), ne svislá jáma.
    magic: bool,
}

impl Portal {
    const fn symbol(self) -> char {
        match (self.up, self.down) {
            (true, true) => '↕',
            (false, true) => '↓',
            (true, false) => '↑',
            (false, false) if self.magic => '*',
            (false, false) => ' ',
        }
    }
}

/// Patro místnosti: 0 les, 1 první podzemí, 2 druhé podzemí.
pub const fn level_of(room: Room) -> Room {
    (room - 1) / LEVEL_SIZE
}

pub const fn level_name(level: Room) -> &'static str {
    LEVELS[level as usize]
}

pub struct Map {
    visited: HashSet<Room>,
    /// Prošlé průchody mezi sousedními místnostmi (menší číslo první).
    passages: HashSet<(Room, Room)>,
    /// Místnosti, odkud nebo kam hráč přešel mezi patry.
    portals: HashMap<Room, Portal>,
    /// Prošlá spojení mezi patry (jámy, studna, kouzelná slova): odkud, kam.
    links: HashSet<(Room, Room)>,
    /// Zdi, do kterých hráč narazil.
    walls: HashSet<(Room, Dir)>,
}

impl Map {
    pub fn new(start: Room) -> Self {
        Self {
            visited: HashSet::from([start]),
            passages: HashSet::new(),
            portals: HashMap::new(),
            links: HashSet::new(),
            walls: HashSet::new(),
        }
    }

    /// Hráč narazil v místnosti do zdi.
    pub fn record_wall(&mut self, room: Room, dir: Dir) {
        self.walls.insert((room, dir));
    }

    /// Ví hráč, že v místnosti tímto směrem vede zeď?
    pub fn knows_wall(&self, room: Room, dir: Dir) -> bool {
        self.walls.contains(&(room, dir))
    }

    /// Zaznamená přesun z `from` do `to`.
    pub fn record(&mut self, from: Room, to: Room) {
        if from == to {
            return;
        }
        self.visited.insert(to);
        let (a, b) = (from.min(to), from.max(to));
        let same_level = level_of(a) == level_of(b);
        let adjacent = (b - a == 1 && (a - 1) % COLUMNS != COLUMNS - 1) || b - a == COLUMNS;
        if same_level && adjacent {
            self.passages.insert((a, b));
            return;
        }
        self.links.insert((from, to));
        // Jámy a studna vedou svisle na stejné místo patra, kouzelná slova jinam.
        let vertical = !same_level && (from - 1) % LEVEL_SIZE == (to - 1) % LEVEL_SIZE;
        let going_down = level_of(to) > level_of(from);
        let from_portal = self.portals.entry(from).or_default();
        if vertical {
            from_portal.down |= going_down;
            from_portal.up |= !going_down;
        } else {
            from_portal.magic = true;
        }
        let to_portal = self.portals.entry(to).or_default();
        if vertical {
            to_portal.up |= going_down;
            to_portal.down |= !going_down;
        } else {
            to_portal.magic = true;
        }
    }

    /// Značka cesty do jiného patra v místnosti, pokud tudy hráč už šel.
    pub fn portal_symbol(&self, room: Room) -> Option<char> {
        self.portals.get(&room).map(|portal| portal.symbol())
    }

    /// Patra, kde hráč byl, v pořadí, v jakém je `render` kreslí vedle sebe.
    pub fn visited_levels(&self) -> Vec<Room> {
        (0..3)
            .filter(|level| (1..=LEVEL_SIZE).any(|i| self.visited.contains(&(level * LEVEL_SIZE + i))))
            .collect()
    }

    /// Do kterých jiných pater vede z místnosti prošlé spojení.
    pub fn connected_levels(&self, room: Room) -> Vec<Room> {
        let mut levels: Vec<Room> = self
            .links
            .iter()
            .filter_map(|&(a, b)| match room {
                _ if a == room => Some(b),
                _ if b == room => Some(a),
                _ => None,
            })
            .map(level_of)
            .filter(|&level| level != level_of(room))
            .collect();
        levels.sort_unstable();
        levels.dedup();
        levels
    }

    /// Místnosti, kde hráč byl.
    pub fn visited(&self) -> impl Iterator<Item = Room> + '_ {
        self.visited.iter().copied()
    }

    /// Všechna navštívená patra vedle sebe (příkaz MAPA, mapa v rozhraní).
    /// `contents` jsou věci v navštívených místnostech, `frame` vybírá, která
    /// z nich se v buňce právě ukazuje (střídají se po sekundě).
    pub fn render(&self, current: Room, scale: Scale, contents: &Contents, frame: usize) -> String {
        let levels = self.visited_levels();
        let grids: Vec<Vec<String>> = levels
            .iter()
            .map(|&level| self.level_lines(level, current, scale, contents, frame))
            .collect();
        let width = scale.level_width();
        let mut lines = vec![
            levels
                .iter()
                .map(|&level| format!("{:width$}", level_name(level)))
                .collect::<Vec<_>>()
                .join(GAP),
        ];
        for row in 0..grids.first().map_or(0, Vec::len) {
            lines.push(grids.iter().map(|grid| grid[row].as_str()).collect::<Vec<_>>().join(GAP));
        }
        lines.into_iter().map(|line| line.trim_end().to_owned()).collect::<Vec<_>>().join("\n")
    }

    /// Jedno patro jako řádky stejné šířky: buňky místností, mezi nimi
    /// průchody a kolem nich zdi, do kterých hráč narazil.
    pub fn level_lines(&self, level: Room, current: Room, scale: Scale, contents: &Contents, frame: usize) -> Vec<String> {
        let (pad, link_len, link_rows) = scale.parts();
        let cell_width = scale.cell_width();
        let mut grid = vec![vec![' '; scale.level_width()]; scale.level_height()];
        for room in level * LEVEL_SIZE + 1..=(level + 1) * LEVEL_SIZE {
            if !self.visited.contains(&room) {
                continue;
            }
            let (y, x) = scale.cell_origin(room);
            let mark = if room == current { '@' } else { self.cycled_symbol(room, contents, frame) };
            let spaces = " ".repeat(pad);
            for (i, c) in format!("[{spaces}{mark}{spaces}]").chars().enumerate() {
                grid[y][x + i] = c;
            }
            if self.passages.contains(&(room, room + 1)) {
                grid[y][x + cell_width..x + cell_width + link_len].fill('─');
            }
            if self.passages.contains(&(room, room + COLUMNS)) {
                for row in &mut grid[y + 1..=y + link_rows] {
                    row[x + pad + 1] = '│';
                }
            }
            for dir in [Dir::North, Dir::East, Dir::South, Dir::West] {
                if !self.knows_wall(room, dir) {
                    continue;
                }
                match dir {
                    Dir::North => grid[y - 1][x..x + cell_width].fill(WALL_HORIZONTAL),
                    Dir::South => grid[y + 1][x..x + cell_width].fill(WALL_HORIZONTAL),
                    Dir::West => grid[y][x - 1] = WALL_VERTICAL,
                    Dir::East => grid[y][x + cell_width] = WALL_VERTICAL,
                }
            }
        }
        grid.into_iter().map(String::from_iter).collect()
    }

    /// Co se v buňce místnosti střídá: značka cesty do jiného patra a ikony věcí.
    fn symbols(&self, room: Room, contents: &Contents) -> Vec<char> {
        let portal = self.portal_symbol(room);
        let items = contents.get(&room).into_iter().flatten().map(|&item| icon(item));
        portal.into_iter().chain(items).collect()
    }

    fn cycled_symbol(&self, room: Room, contents: &Contents, frame: usize) -> char {
        let symbols = self.symbols(room, contents);
        if symbols.is_empty() { ' ' } else { symbols[frame % symbols.len()] }
    }

    /// Kolik snímků je potřeba, aby se vystřídaly všechny místnosti
    /// (nejmenší společný násobek počtů, nejvýš `MAX_FRAMES`).
    pub fn frame_count(&self, contents: &Contents) -> usize {
        self.visited
            .iter()
            .map(|&room| self.symbols(room, contents).len().max(1))
            .fold(1, |acc, n| (acc * n / gcd(acc, n)).min(MAX_FRAMES))
    }
}

const fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

const MAX_FRAMES: usize = 60;

/// Věci v navštívených místnostech.
pub type Contents = HashMap<Room, Vec<Item>>;

/// Krátký název věci do legendy mapy (1. pád; herní texty používají 4. pád).
pub const fn label(item: Item) -> &'static str {
    match item {
        Item::Diamond => "diamant",
        Item::Spoon => "lžička",
        Item::Coins => "zlaťáky",
        Item::Watch => "hodinky",
        Item::Platinum => "platina",
        Item::Snake => "had",
        Item::Door => "dveře",
        Item::Lamp => "svítilna",
        Item::LitLamp => "rozsvícená svítilna",
        Item::Rifle => "puška",
        Item::Bird => "ptáček",
        Item::Key => "klíč",
        Item::Well => "studna",
        Item::Spacesuit => "skafandr",
        Item::Ammo => "náboje",
        Item::Magpie => "straka",
        Item::Battery => "baterie",
        Item::Lion => "lev",
        Item::Poison => "jed",
        Item::Skeleton => "kostra",
        Item::Inscription => "nápis",
        Item::Dragon => "drak",
        Item::Cutters => "kleště",
        Item::Screwdriver => "šroubovák",
        Item::Flowerpot => "květináč",
    }
}

/// Ikona věci v mapě a v panelech: jeden úzký znak, který věc připomíná.
pub const fn icon(item: Item) -> char {
    match item {
        Item::Diamond => '◆',
        Item::Spoon => 'ǫ',
        Item::Coins => '¤',
        Item::Watch => '◷',
        Item::Platinum => '▪',
        Item::Snake => 'ʃ',
        Item::Door => 'Π',
        Item::Lamp => '○',
        Item::LitLamp => '☼',
        Item::Rifle => '⌐',
        Item::Bird => 'ʋ',
        Item::Key => 'ꝃ',
        Item::Well => 'ʊ',
        Item::Spacesuit => 'Ω',
        Item::Ammo => '⁞',
        Item::Magpie => 'Ʋ',
        Item::Battery => '⊟',
        Item::Lion => 'ℓ',
        Item::Poison => '☠',
        Item::Skeleton => '†',
        Item::Inscription => '¶',
        Item::Dragon => 'Ʒ',
        Item::Cutters => 'λ',
        Item::Screwdriver => 'Ŧ',
        Item::Flowerpot => 'Ψ',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_only_walked_paths() {
        let mut map = Map::new(7);
        map.record(7, 8);
        map.record(8, 12);
        let text = map.render(12, Scale::Small, &Contents::new(), 0);
        let lines: Vec<&str> = text.lines().collect();
        // Řádek 0 je název patra, řádek 1 horní okraj, pak řady místností.
        assert_eq!(lines[0].trim_end(), "les");
        assert_eq!(lines[4], "         [ ]─[ ]");
        assert_eq!(lines[5], "              │");
        assert_eq!(lines[6], "             [@]");
        assert!(!text.contains("podzemí"));

        let large = map.render(12, Scale::Large, &Contents::new(), 0);
        let lines: Vec<&str> = large.lines().collect();
        assert_eq!(lines[5], "                 [   ]───[   ]");
        assert_eq!(lines[6], "                           │");
        assert_eq!(lines[7], "                           │");
        assert_eq!(lines[8], "                         [ @ ]");
    }

    #[test]
    fn walls_are_drawn_on_the_right_side() {
        // Místnost 1 vlevo nahoře: zeď na severu i na západě leží na okraji.
        let mut map = Map::new(1);
        map.record_wall(1, Dir::North);
        map.record_wall(1, Dir::West);
        map.record_wall(1, Dir::East);
        map.record_wall(1, Dir::South);
        let lines = map.level_lines(0, 1, Scale::Small, &Contents::new(), 0);
        assert_eq!(lines[0].trim_end(), " ═══");
        assert_eq!(lines[1].trim_end(), "║[@]║");
        assert_eq!(lines[2].trim_end(), " ═══");
        assert!(map.knows_wall(1, Dir::North));
        assert!(!map.knows_wall(2, Dir::North));
    }

    #[test]
    fn levels_side_by_side_stay_aligned() {
        let mut map = Map::new(2);
        map.record(2, 22);
        map.record(22, 26);
        // Průchod z 22 do 26 vede ve druhém sloupci druhého zobrazeného patra:
        // svislá čára musí být přesně pod středem buňky místnosti 22.
        for (scale, center) in [(Scale::Small, 6), (Scale::Large, 11)] {
            let text = map.render(26, scale, &Contents::new(), 0);
            let lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
            let column = scale.level_width() + GAP.len() + center;
            assert_eq!(lines[2].get(column), Some(&'↑'), "{text}");
            assert_eq!(lines[3].get(column), Some(&'│'), "{text}");
            assert_eq!(lines[2].get(column - 1), Some(&if center == 6 { '[' } else { ' ' }), "{text}");
        }
    }

    #[test]
    fn items_and_shaft_take_turns_in_the_cell() {
        let mut map = Map::new(2);
        map.record(2, 22);
        // Věci v místnosti, kde hráč stojí, konzole do mapy nedává (tam je jen @).
        let contents = Contents::from([(2, vec![Item::Diamond, Item::Snake])]);
        // V místnosti 2 se střídá jáma, diamant a had; v 22 stojí hráč.
        assert_eq!(map.frame_count(&contents), 3);
        let cells: Vec<String> = (0..3)
            .map(|frame| map.render(22, Scale::Small, &contents, frame).lines().nth(2).unwrap().to_owned())
            .collect();
        assert!(cells[0].contains("[↓]"), "{cells:?}");
        assert!(cells[1].contains("[◆]"), "{cells:?}");
        assert!(cells[2].contains("[ʃ]"), "{cells:?}");
        assert!(cells.iter().all(|line| line.contains("[@]")), "{cells:?}");
    }

    #[test]
    fn levels_are_connected_where_player_went_between_them() {
        let mut map = Map::new(2);
        map.record(2, 22);
        map.record(22, 42);
        map.record(42, 22);
        assert_eq!(map.connected_levels(2), [1]);
        assert_eq!(map.connected_levels(22), [0, 2]);
        assert_eq!(map.connected_levels(42), [1]);
        // BALALAJKA vede v rámci stejného patra, přepínat není kam.
        map.record(53, 43);
        assert!(map.connected_levels(53).is_empty());
        // Klik na buňku místnosti 22 v prvním podzemí.
        let (y, x) = Scale::Large.cell_origin(22);
        assert_eq!(Scale::Large.room_at(1, y, x + 2), Some(22));
        assert_eq!(Scale::Large.room_at(1, y + 1, x), None);
    }

    #[test]
    fn row_edge_is_not_a_passage() {
        let mut map = Map::new(4);
        map.record(4, 5);
        assert!(map.passages.is_empty());
        assert_eq!(map.portal_symbol(4), Some('*'));
    }

    #[test]
    fn shaft_marks_direction_at_both_ends() {
        let mut map = Map::new(2);
        map.record(2, 22);
        let text = map.render(22, Scale::Small, &Contents::new(), 0);
        assert!(text.contains("1. podzemí"), "{text}");
        assert!(text.contains("[↓]"), "{text}");
        assert!(text.contains("[@]"), "{text}");
        map.record(22, 42);
        assert_eq!(map.portal_symbol(22), Some('↕'));
        assert_eq!(map.portal_symbol(42), Some('↑'));
    }

    #[test]
    fn magic_words_are_not_vertical_shafts() {
        // ALLAH ve 32 přenese do lesa na 20, odtud vede studna dolů na 60.
        let mut map = Map::new(32);
        map.record(32, 20);
        assert_eq!(map.portal_symbol(32), Some('*'));
        assert_eq!(map.portal_symbol(20), Some('*'));
        map.record(20, 60);
        assert_eq!(map.portal_symbol(20), Some('↓'));
        assert_eq!(map.portal_symbol(60), Some('↑'));
    }
}
