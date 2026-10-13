//! Herní data: mapa, předměty a nápovědy.
//!
//! Hodnoty pocházejí z původního programu (`analyza/poklad2.bas`, řádky 9000-9038).

/// Číslo místnosti 1-60: 1-20 les, 21-40 první podzemí, 41-60 druhé podzemí.
pub type Room = u8;

pub const START_ROOM: Room = 7;
pub const LAST_FOREST_ROOM: Room = 20;
pub const ALLAH_ROOM: Room = 32;
pub const RUSSIAN_ROOM: Room = 53;
pub const DEATH_CELL: Room = 43;

/// Každé patro je mřížka 4 × 5 místností.
const ROW_WIDTH: Room = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    const fn wall_bit(self) -> u8 {
        match self {
            Self::North => 0b1000,
            Self::East => 0b0100,
            Self::South => 0b0010,
            Self::West => 0b0001,
        }
    }
}

/// Zdi místností 1-60, bity v pořadí sever, východ, jih, západ.
/// Kontroluje se jen zeď v místnosti, ze které se odchází (stejně jako v originálu).
#[rustfmt::skip]
const WALLS: [u8; 60] = [
    // les
    0b1001, 0b1010, 0b1010, 0b1110,
    0b0101, 0b1001, 0b1000, 0b1100,
    0b0011, 0b0110, 0b0101, 0b0101,
    0b1001, 0b1100, 0b0111, 0b0101,
    0b0111, 0b0011, 0b1010, 0b0110,
    // první podzemí
    0b1101, 0b1101, 0b1011, 0b1100,
    0b0101, 0b0011, 0b1100, 0b0101,
    0b0101, 0b1101, 0b0001, 0b0100,
    0b0101, 0b0011, 0b0100, 0b0101,
    0b0011, 0b1010, 0b0110, 0b0111,
    // druhé podzemí
    0b1101, 0b1111, 0b1111, 0b1100,
    0b0101, 0b1101, 0b1101, 0b0101,
    0b0001, 0b0110, 0b0101, 0b0101,
    0b0001, 0b1010, 0b0100, 0b0101,
    0b0111, 0b1011, 0b0010, 0b0110,
];

/// Sousední místnost ve směru `dir`, pokud tam nevede zeď.
pub fn neighbor(room: Room, dir: Dir) -> Option<Room> {
    if WALLS[usize::from(room - 1)] & dir.wall_bit() != 0 {
        return None;
    }
    // Okraje pater jsou vždy zazděné, takže výsledek zůstane v rozsahu 1-60.
    Some(match dir {
        Dir::North => room - ROW_WIDTH,
        Dir::East => room + 1,
        Dir::South => room + ROW_WIDTH,
        Dir::West => room - 1,
    })
}

/// Kam vede jáma nahoru.
pub const fn shaft_up(room: Room) -> Option<Room> {
    match room {
        22 => Some(2),
        42 => Some(22),
        _ => None,
    }
}

/// Kam vede jáma (nebo studna) dolů.
pub const fn shaft_down(room: Room) -> Option<Room> {
    match room {
        2 => Some(22),
        22 => Some(42),
        20 => Some(60),
        _ => None,
    }
}

/// Místnosti, ve kterých je vidět jáma.
pub const fn has_pit(room: Room) -> bool {
    matches!(room, 2 | 22 | 42)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Diamond,
    Spoon,
    Coins,
    Watch,
    Platinum,
    Snake,
    Door,
    Lamp,
    LitLamp,
    Rifle,
    Bird,
    Key,
    Well,
    Spacesuit,
    Ammo,
    Magpie,
    Battery,
    Lion,
    Poison,
    Skeleton,
    Inscription,
    Dragon,
    Cutters,
    Screwdriver,
    Flowerpot,
}

pub struct ItemDef {
    pub item: Item,
    /// Název ve 4. pádě, jak se vypisuje za "Vidíš:".
    pub name: &'static str,
    /// Výchozí místnost, `None` = předmět ve hře zatím není.
    pub start: Option<Room>,
    pub portable: bool,
}

const fn def(item: Item, name: &'static str, start: Option<Room>, portable: bool) -> ItemDef {
    ItemDef { item, name, start, portable }
}

/// Pořadí odpovídá originálu, na něm závisí i pořadí výpisu předmětů.
pub const ITEMS: [ItemDef; 25] = [
    def(Item::Diamond, "diamant", Some(4), true),
    def(Item::Spoon, "stříbrnou lžičku", None, true),
    def(Item::Coins, "pytlík zlaťáků", Some(29), true),
    def(Item::Watch, "švýcarské hodinky", Some(47), true),
    def(Item::Platinum, "kousek platiny", Some(41), true),
    def(Item::Snake, "hada", Some(3), false),
    def(Item::Door, "zamčené dveře", Some(5), false),
    def(Item::Lamp, "zhasnutou svítilnu", Some(8), true),
    def(Item::LitLamp, "rozsvícenou svítilnu", None, true),
    def(Item::Rifle, "pušku", Some(14), true),
    def(Item::Bird, "ptáčka v kleci", Some(15), true),
    def(Item::Key, "klíč", Some(17), true),
    def(Item::Well, "studnu", Some(20), false),
    def(Item::Spacesuit, "skafandr", Some(21), true),
    def(Item::Ammo, "náboje", Some(23), true),
    def(Item::Magpie, "straku", Some(27), false),
    def(Item::Battery, "baterie", Some(30), true),
    def(Item::Lion, "lva", Some(33), false),
    def(Item::Poison, "lahvičku s jedem", Some(40), true),
    def(Item::Skeleton, "lidskou kostru", Some(43), false),
    def(Item::Inscription, "nápis na zdi:'NEVSTUPOVAT'", Some(44), false),
    def(Item::Dragon, "draka", Some(45), false),
    def(Item::Cutters, "štípací kleště", Some(46), true),
    def(Item::Screwdriver, "šroubovák", Some(56), true),
    def(Item::Flowerpot, "květináč", Some(58), true),
];

impl Item {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn def(self) -> &'static ItemDef {
        &ITEMS[self.index()]
    }

    pub const fn name(self) -> &'static str {
        self.def().name
    }

    /// Prvních pět předmětů jsou poklady.
    pub const fn is_treasure(self) -> bool {
        self.index() < 5
    }
}

/// Nápověda pro příkaz POMOC podle místnosti. Mezery uvnitř jsou z originálu
/// (zarovnání na 32 sloupců).
pub const fn hint(room: Room) -> &'static str {
    match room {
        2 => "V podzemí bude asi tma...",
        3 => "Znám zvíře, které by mohlo hada sníst...",
        5 => "Zamčené dveře ti brání v cestě  na sever...",
        20 => "A co takhle skafandr...?",
        27 => "Straky kradou drahé kovy, co    když má tahle něco v žaludku...?",
        33 => "To si nevíš rady v takové       klasické situaci...?",
        42 => "Do druhého podzemí se asi budeš muset dostat jinudy...",
        43 => "To máš za to...!",
        44 => "Já bych tam nechodil...",
        45 => "Drak sežere, co mu podstrčíš...",
        53 => "Já bych si dal pozor...",
        _ => DEFAULT_HINT,
    }
}

/// Rada, když místnost nic zvláštního nemá (nebo je překážka vyřešená).
pub const DEFAULT_HINT: &str = "Počínáš si dobře...";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_in_declaration_order() {
        for (i, def) in ITEMS.iter().enumerate() {
            assert_eq!(def.item.index(), i, "{}", def.name);
        }
    }

    #[test]
    fn edges_are_walled() {
        for room in 1..=60 {
            let column = (room - 1) % ROW_WIDTH;
            let row = (room - 1) / ROW_WIDTH % 5;
            if column == 0 {
                assert_eq!(neighbor(room, Dir::West), None, "{room}");
            }
            if column == ROW_WIDTH - 1 {
                assert_eq!(neighbor(room, Dir::East), None, "{room}");
            }
            if row == 0 {
                assert_eq!(neighbor(room, Dir::North), None, "{room}");
            }
            if row == 4 {
                assert_eq!(neighbor(room, Dir::South), None, "{room}");
            }
        }
    }
}
