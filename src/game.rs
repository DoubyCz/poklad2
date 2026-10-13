//! Herní stav a pravidla. Nic tu nečte z klávesnice ani nepíše na obrazovku,
//! jen vrací texty, takže jde celá hra projít v testech.

use std::fmt::Write as _;

use crate::data::{
    self, ALLAH_ROOM, DEATH_CELL, Dir, ITEMS, Item, LAST_FOREST_ROOM, RUSSIAN_ROOM, Room,
    START_ROOM,
};

const MAX_LOAD: usize = 5;
const TREASURE_GOAL: u8 = 5;
/// Na kolik tahů vydrží svítilna bez náhradní baterie.
const LAMP_CHARGE: i32 = 100;

// Všechny hlášky jsou doslova z originálu, včetně interpunkce a mezer, kterými
// Fuka zarovnával text na 32 sloupců Spectra (UI je podle toho láme).
pub const CANT_GO: &str = "Tam se nedá jít";
pub const NOT_YET: &str = "Prozatím tam nemůžeš";
pub const CANT_DO: &str = "To nemůžeš";
pub const NOT_HERE: &str = "Není tady";
pub const TOO_MANY: &str = "Tak mnoho neuneseš";
pub const UNKNOWN_COMMAND: &str = "Tento příkaz neznám";
pub const UNKNOWN_ITEM: &str = "Tento předmět neznám";
pub const DEATH: &str = "Máš vybité baterie ve svítilně  nebo zhasnuto.\n       Jsi MRTEV !";
pub const SEEN: &str = "Vidíš:";
/// Jediný nový text oproti originálu (Jirkovo rozhodnutí): odemčené dveře.
pub const UNLOCKED_DOOR: &str = "odemčené dveře";
pub const NOTHING_SEEN: &str = "Nevidíš nic zvláštního.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Room(Room),
    Carried,
    /// Předmět ve hře (zatím nebo už) není.
    Gone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Go(Dir),
    Up,
    Down,
    Take(Item),
    Drop(Item),
    Shoot,
    Inventory,
    Light,
    Balalajka,
    Allah,
    Help,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    Silent,
    Message(String),
    Won,
}

impl From<&str> for Response {
    fn from(text: &str) -> Self {
        Self::Message(text.to_owned())
    }
}

/// Popis místnosti na začátku tahu.
pub struct Turn {
    /// "Jsi ..." a případné zvláštnosti místnosti.
    pub location: String,
    /// Co tu leží; vypisuje se pod `SEEN` nebo místo toho `NOTHING_SEEN`.
    pub items: Vec<Item>,
    /// `false` = hráč ve tmě zemřel, vypisuje se `DEATH`.
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    room: Room,
    turns: u32,
    charge: i32,
    places: [Place; ITEMS.len()],
    treasures: u8,
    /// Dveře odemčené klíčem. V originálu tenhle stav není (dveře zůstávají
    /// "zamčené" a pokaždé se kontroluje klíč); přidáno na Jirkovo přání.
    door_unlocked: bool,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Self {
            room: START_ROOM,
            turns: 0,
            charge: 0,
            places: ITEMS.map(|def| def.start.map_or(Place::Gone, Place::Room)),
            treasures: 0,
            door_unlocked: false,
        }
    }

    pub const fn turns(&self) -> u32 {
        self.turns
    }

    /// Kolik pokladů už hráč našel.
    pub const fn treasures(&self) -> u8 {
        self.treasures
    }

    pub const fn place(&self, item: Item) -> Place {
        self.places[item.index()]
    }

    fn set(&mut self, item: Item, place: Place) {
        self.places[item.index()] = place;
    }

    fn carries(&self, item: Item) -> bool {
        self.place(item) == Place::Carried
    }

    fn is_here(&self, item: Item) -> bool {
        self.place(item) == Place::Room(self.room)
    }

    fn load(&self) -> usize {
        self.places.iter().filter(|&&p| p == Place::Carried).count()
    }

    fn items_where(&self, place: Place) -> impl Iterator<Item = Item> + '_ {
        ITEMS.iter().map(|def| def.item).filter(move |&item| self.place(item) == place)
    }

    /// Začne nový tah: popíše místnost a zkontroluje, jestli hráč ve tmě nezemřel.
    pub fn begin_turn(&mut self) -> Turn {
        self.turns += 1;
        let mut text = String::from(match self.room {
            ..=LAST_FOREST_ROOM => "Jsi v lese.",
            21..=40 => "Jsi v prvním podzemí.",
            _ => "Jsi ve druhém podzemí.",
        });
        if data::has_pit(self.room) {
            text.push_str("\nJe tu jáma.");
        }
        // Originál tyhle řádky vypisuje pod patro, i když navazují na "Jsi".
        match self.room {
            DEATH_CELL => text.push_str("\nv kobce smrti\nnení úniku..."),
            ALLAH_ROOM => text.push_str("\nv zázračné komnatě\nřekni ALLAH"),
            RUSSIAN_ROOM => text.push_str("\nv ruské komnatě\nřekni BALALAJKA"),
            _ => {}
        }

        if !self.carries(Item::Battery) {
            self.charge = self.charge.saturating_sub(1);
        }
        let underground = self.room > LAST_FOREST_ROOM;
        let alive = !underground || (self.carries(Item::LitLamp) && self.charge >= 0);
        let items = if alive { self.items_where(Place::Room(self.room)).collect() } else { Vec::new() };
        Turn { location: text, items, alive }
    }

    pub const fn room(&self) -> Room {
        self.room
    }

    /// Předměty v místnosti (pro tlačítka a doplňování v rozhraní).
    pub fn items_here(&self) -> Vec<Item> {
        self.items_where(Place::Room(self.room)).collect()
    }

    /// Rada pro místnost (POMOC). Originál radil podle místnosti vždy; tady
    /// rada zmizí, když je překážka vyřešená (Jirkovo přání).
    pub fn hint(&self) -> &'static str {
        let solved = match self.room {
            2 => self.carries(Item::LitLamp),
            3 => !self.is_here(Item::Snake),
            5 => self.door_unlocked,
            20 => self.carries(Item::Spacesuit),
            27 => !self.is_here(Item::Magpie),
            33 => !self.is_here(Item::Lion),
            45 => !self.is_here(Item::Dragon),
            _ => false,
        };
        if solved { data::DEFAULT_HINT } else { data::hint(self.room) }
    }

    /// Název předmětu ve 4. pádě, jak se vypisuje; jen dveře se po odemčení
    /// jmenují jinak.
    pub const fn item_name(&self, item: Item) -> &'static str {
        match item {
            Item::Door if self.door_unlocked => UNLOCKED_DOOR,
            _ => item.name(),
        }
    }

    /// Předměty v libovolné místnosti (pro mapu navštívených míst).
    pub fn items_at(&self, room: Room) -> Vec<Item> {
        self.items_where(Place::Room(room)).collect()
    }

    /// Předměty, které hráč nese.
    pub fn items_carried(&self) -> Vec<Item> {
        self.items_where(Place::Carried).collect()
    }

    pub fn execute(&mut self, action: Action) -> Response {
        match action {
            Action::Go(dir) => self.go(dir),
            Action::Up => self.climb(data::shaft_up(self.room)),
            Action::Down => self.descend(),
            Action::Take(item) => self.take(item),
            Action::Drop(item) => self.drop(item),
            Action::Shoot => self.shoot(),
            Action::Inventory => self.inventory(),
            Action::Light => self.light(),
            Action::Balalajka => self.magic_word(RUSSIAN_ROOM, DEATH_CELL),
            Action::Allah => self.magic_word(ALLAH_ROOM, LAST_FOREST_ROOM),
            Action::Help => self.hint().into(),
        }
    }

    fn go(&mut self, dir: Dir) -> Response {
        let Some(target) = data::neighbor(self.room, dir) else {
            return CANT_GO.into();
        };
        let blocked = match dir {
            Dir::North => {
                self.is_here(Item::Lion)
                    || self.is_here(Item::Dragon)
                    || (self.is_here(Item::Door) && !self.door_unlocked && !self.carries(Item::Key))
            }
            Dir::East => self.is_here(Item::Snake),
            Dir::South | Dir::West => false,
        };
        if blocked {
            return NOT_YET.into();
        }
        // Kdo projde dveřmi s klíčem, ten je odemkne natrvalo.
        if dir == Dir::North && self.is_here(Item::Door) {
            self.door_unlocked = true;
        }
        self.climb(Some(target))
    }

    fn climb(&mut self, target: Option<Room>) -> Response {
        match target {
            Some(room) => {
                self.room = room;
                Response::Silent
            }
            None => CANT_GO.into(),
        }
    }

    fn descend(&mut self) -> Response {
        if self.is_here(Item::Well) && !self.carries(Item::Spacesuit) {
            return NOT_YET.into();
        }
        self.climb(data::shaft_down(self.room))
    }

    fn take(&mut self, item: Item) -> Response {
        if !item.def().portable {
            return CANT_DO.into();
        }
        if !self.is_here(item) {
            return NOT_HERE.into();
        }
        if item.is_treasure() {
            self.set(item, Place::Gone);
            self.treasures += 1;
            return if self.treasures == TREASURE_GOAL { Response::Won } else { Response::Silent };
        }
        if self.load() >= MAX_LOAD {
            return TOO_MANY.into();
        }
        self.set(item, Place::Carried);
        Response::Silent
    }

    fn drop(&mut self, item: Item) -> Response {
        if !self.carries(item) {
            return CANT_DO.into();
        }
        self.set(item, Place::Room(self.room));
        // Ptáček sežere hada, jed otráví draka.
        if item == Item::Bird && self.is_here(Item::Snake) {
            self.set(Item::Snake, Place::Gone);
        }
        if item == Item::Poison && self.is_here(Item::Dragon) {
            self.set(Item::Dragon, Place::Gone);
        }
        Response::Silent
    }

    fn shoot(&mut self) -> Response {
        if !self.carries(Item::Rifle) {
            return "Nemáš pušku".into();
        }
        if !self.carries(Item::Ammo) {
            return "Nemáš náboje".into();
        }
        if self.is_here(Item::Magpie) {
            self.set(Item::Magpie, Place::Gone);
            self.set(Item::Spoon, Place::Room(self.room));
        } else if self.is_here(Item::Lion) {
            self.set(Item::Lion, Place::Gone);
        } else {
            return CANT_DO.into();
        }
        Response::Silent
    }

    fn inventory(&self) -> Response {
        let mut text = format!("Bloudíš už {} hodin.\nMáš {} pokladů", self.turns, self.treasures);
        for item in self.items_where(Place::Carried) {
            let _ = write!(text, "\n  {}", self.item_name(item));
        }
        Response::Message(text)
    }

    fn light(&mut self) -> Response {
        if !self.carries(Item::Lamp) {
            return "Nemáš svítilnu".into();
        }
        self.set(Item::Lamp, Place::Gone);
        self.set(Item::LitLamp, Place::Carried);
        self.charge = LAMP_CHARGE;
        Response::Silent
    }

    fn magic_word(&mut self, room: Room, target: Room) -> Response {
        if self.room != room {
            return UNKNOWN_COMMAND.into();
        }
        self.room = target;
        Response::Silent
    }

    /// Uložená pozice jako text, jeden údaj na řádek.
    pub fn to_save(&self) -> String {
        let places: Vec<String> = self
            .places
            .iter()
            .map(|place| match place {
                Place::Room(room) => room.to_string(),
                Place::Carried => "C".to_owned(),
                Place::Gone => "-".to_owned(),
            })
            .collect();
        format!(
            "poklad2 2\nroom {}\nturns {}\ncharge {}\ntreasures {}\nplaces {}\ndoor {}\n",
            self.room,
            self.turns,
            self.charge,
            self.treasures,
            places.join(" "),
            u8::from(self.door_unlocked),
        )
    }

    pub fn from_save(text: &str) -> Option<Self> {
        let mut lines = text.lines().map(|line| line.split_once(' '));
        let mut field = |name: &str| match lines.next()? {
            Some((key, value)) if key == name => Some(value),
            _ => None,
        };
        // Verze 1 ještě neznala odemčené dveře.
        let version = field("poklad2")?;
        if version != "1" && version != "2" {
            return None;
        }
        let room: Room = field("room")?.parse().ok().filter(|r| (1..=60).contains(r))?;
        let turns = field("turns")?.parse().ok()?;
        let charge = field("charge")?.parse().ok()?;
        let treasures = field("treasures")?.parse().ok().filter(|&t| t < TREASURE_GOAL)?;
        let places: Vec<Place> = field("places")?
            .split(' ')
            .map(|p| match p {
                "C" => Some(Place::Carried),
                "-" => Some(Place::Gone),
                _ => p.parse().ok().filter(|r| (1..=60).contains(r)).map(Place::Room),
            })
            .collect::<Option<_>>()?;
        let door_unlocked = version == "2" && field("door")? == "1";
        Some(Self { room, turns, charge, places: places.try_into().ok()?, treasures, door_unlocked })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(room: Room) -> Game {
        Game { room, ..Game::new() }
    }

    #[test]
    fn walls_block() {
        let mut game = Game::new();
        assert_eq!(game.execute(Action::Go(Dir::North)), CANT_GO.into());
        assert_eq!(game.execute(Action::Go(Dir::East)), Response::Silent);
        assert_eq!(game.room, 8);
    }

    #[test]
    fn snake_blocks_east_until_bird_is_dropped() {
        let mut game = at(3);
        assert_eq!(game.execute(Action::Go(Dir::East)), NOT_YET.into());
        game.set(Item::Bird, Place::Carried);
        assert_eq!(game.execute(Action::Drop(Item::Bird)), Response::Silent);
        assert_eq!(game.place(Item::Snake), Place::Gone);
        assert_eq!(game.execute(Action::Go(Dir::East)), Response::Silent);
    }

    #[test]
    fn door_needs_key() {
        let mut game = at(5);
        assert_eq!(game.execute(Action::Go(Dir::North)), NOT_YET.into());
        game.set(Item::Key, Place::Carried);
        assert_eq!(game.execute(Action::Go(Dir::North)), Response::Silent);
        assert_eq!(game.room, 1);
    }

    #[test]
    fn door_stays_unlocked_after_passing_with_key() {
        let mut game = at(5);
        game.set(Item::Key, Place::Carried);
        assert_eq!(game.item_name(Item::Door), "zamčené dveře");
        assert_eq!(game.execute(Action::Go(Dir::North)), Response::Silent);
        assert_eq!(game.item_name(Item::Door), UNLOCKED_DOOR);
        // Bez klíče už projde taky.
        game.execute(Action::Go(Dir::South));
        game.set(Item::Key, Place::Room(5));
        assert_eq!(game.execute(Action::Go(Dir::North)), Response::Silent);
        assert_eq!(Game::from_save(&game.to_save()), Some(game));
    }

    #[test]
    fn hint_disappears_when_obstacle_is_solved() {
        let mut game = at(3);
        assert_eq!(game.hint(), data::hint(3));
        game.set(Item::Bird, Place::Carried);
        game.execute(Action::Drop(Item::Bird));
        assert_eq!(game.hint(), data::DEFAULT_HINT);
        assert_eq!(at(43).hint(), data::hint(43));
    }

    #[test]
    fn underground_without_light_kills() {
        let mut game = at(2);
        assert_eq!(game.execute(Action::Down), Response::Silent);
        assert!(!game.begin_turn().alive);
    }

    #[test]
    fn lamp_runs_out_without_battery() {
        let mut game = at(22);
        game.set(Item::Lamp, Place::Carried);
        game.execute(Action::Light);
        for _ in 0..LAMP_CHARGE {
            assert!(game.begin_turn().alive);
        }
        assert!(!game.begin_turn().alive);
    }

    #[test]
    fn battery_keeps_lamp_alive() {
        let mut game = at(22);
        game.set(Item::Lamp, Place::Carried);
        game.set(Item::Battery, Place::Carried);
        game.execute(Action::Light);
        for _ in 0..1000 {
            assert!(game.begin_turn().alive);
        }
    }

    #[test]
    fn load_limit_ignores_treasures() {
        let mut game = at(4);
        for item in [Item::Lamp, Item::Rifle, Item::Key, Item::Bird, Item::Ammo] {
            game.set(item, Place::Carried);
        }
        assert_eq!(game.execute(Action::Take(Item::Diamond)), Response::Silent);
        game.set(Item::Cutters, Place::Room(4));
        assert_eq!(game.execute(Action::Take(Item::Cutters)), TOO_MANY.into());
    }

    #[test]
    fn shooting_magpie_drops_spoon() {
        let mut game = at(27);
        game.set(Item::Rifle, Place::Carried);
        assert_eq!(game.execute(Action::Shoot), "Nemáš náboje".into());
        game.set(Item::Ammo, Place::Carried);
        assert_eq!(game.execute(Action::Shoot), Response::Silent);
        assert_eq!(game.place(Item::Spoon), Place::Room(27));
    }

    #[test]
    fn balalajka_leads_to_death_cell() {
        let mut game = at(RUSSIAN_ROOM);
        game.execute(Action::Balalajka);
        assert_eq!(game.room, DEATH_CELL);
        assert_eq!(game.execute(Action::Balalajka), UNKNOWN_COMMAND.into());
    }

    #[test]
    fn save_roundtrip() {
        let mut game = at(30);
        game.turns = 42;
        game.charge = 17;
        game.treasures = 3;
        game.set(Item::Battery, Place::Carried);
        game.set(Item::Diamond, Place::Gone);
        assert_eq!(Game::from_save(&game.to_save()), Some(game));
        assert_eq!(Game::from_save("nesmysl"), None);
    }
}
