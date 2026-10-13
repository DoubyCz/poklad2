//! Rozbor příkazů z klávesnice.
//!
//! Stejně jako v originálu rozhodují první tři písmena slova, takže `VEZ`,
//! `VEZMI` i `VEZMĚTE` znamenají totéž. Navíc jde napsat příkaz i s předmětem
//! na jeden řádek (`vezmi diamant`), diakritika a velikost písmen nehrají roli.

use crate::data::{Dir, ITEMS, Item};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Go(Dir),
    Up,
    Down,
    Take,
    Drop,
    Shoot,
    Inventory,
    Light,
    Balalajka,
    Allah,
    Help,
    Quit,
    Save,
    Load,
}

/// Slova příkazů. Jednopísmenná se musí shodovat celá, ostatní v prvních třech písmenech.
const VERBS: [(Verb, &[&str]); 17] = [
    (Verb::Go(Dir::North), &["S", "SEVER"]),
    (Verb::Go(Dir::South), &["J", "JIH"]),
    (Verb::Go(Dir::West), &["Z", "ZAPAD"]),
    (Verb::Go(Dir::East), &["V", "VYCHOD"]),
    (Verb::Up, &["N", "NAHORU"]),
    (Verb::Down, &["D", "DOLU"]),
    (Verb::Take, &["VEZMI"]),
    (Verb::Drop, &["POLOZ"]),
    (Verb::Shoot, &["ZASTREL"]),
    (Verb::Inventory, &["INVENTURA"]),
    (Verb::Light, &["ROZSVIT"]),
    (Verb::Balalajka, &["BALALAJKA"]),
    (Verb::Allah, &["ALLAH"]),
    (Verb::Help, &["POMOC"]),
    (Verb::Quit, &["KONEC"]),
    (Verb::Save, &["SAVE"]),
    (Verb::Load, &["LOAD"]),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub verb: Verb,
    /// Zbytek řádku za příkazem (předmět), už normalizovaný.
    pub object: Option<String>,
}

/// Převede text na velká písmena bez diakritiky.
pub fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'Á' => 'A',
            'č' | 'Č' => 'C',
            'ď' | 'Ď' => 'D',
            'é' | 'É' | 'ě' | 'Ě' => 'E',
            'í' | 'Í' => 'I',
            'ň' | 'Ň' => 'N',
            'ó' | 'Ó' => 'O',
            'ř' | 'Ř' => 'R',
            'š' | 'Š' => 'S',
            'ť' | 'Ť' => 'T',
            'ú' | 'Ú' | 'ů' | 'Ů' => 'U',
            'ý' | 'Ý' => 'Y',
            'ž' | 'Ž' => 'Z',
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

fn prefix3(word: &str) -> &str {
    word.char_indices().nth(3).map_or(word, |(i, _)| &word[..i])
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())
}

/// Rozebere řádek na příkaz. `None` = neznámý příkaz (nebo prázdný řádek).
pub fn parse(line: &str) -> Option<Command> {
    let line = normalize(line);
    let mut parts = words(&line);
    let first = parts.next()?;
    let verb = VERBS.iter().find_map(|&(verb, keys)| {
        keys.iter()
            .any(|key| if key.len() == 1 { first == *key } else { prefix3(first) == prefix3(key) })
            .then_some(verb)
    })?;
    let object: Vec<&str> = parts.collect();
    Some(Command { verb, object: (!object.is_empty()).then(|| object.join(" ")) })
}

/// Najde předmět podle slova. Stačí první tři písmena kteréhokoliv slova názvu
/// (originál znal jen první slovo). Při shodě vyhraje ten, jehož slovo celé
/// začíná zadaným textem, pak shoda v prvním slově názvu a pak pořadí v seznamu.
pub fn find_item(text: &str) -> Option<Item> {
    let text = normalize(text);
    let word = words(&text).next()?;
    let mut best: Option<(Item, (bool, bool))> = None;
    for def in &ITEMS {
        let name = normalize(def.name);
        let score = words(&name)
            .enumerate()
            .filter(|(_, w)| prefix3(w) == prefix3(word))
            .map(|(pos, w)| (w.starts_with(word), pos == 0))
            .max();
        if let Some(score) = score
            && best.is_none_or(|(_, b)| score > b)
        {
            best = Some((def.item, score));
        }
    }
    best.map(|(item, _)| item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verb(line: &str) -> Option<Verb> {
        parse(line).map(|c| c.verb)
    }

    #[test]
    fn verbs_match_first_three_letters() {
        assert_eq!(verb("vez"), Some(Verb::Take));
        assert_eq!(verb("VEZMI"), Some(Verb::Take));
        assert_eq!(verb("položit"), Some(Verb::Drop));
        assert_eq!(verb("ve"), None);
        assert_eq!(verb("s"), Some(Verb::Go(Dir::North)));
        assert_eq!(verb("sever"), Some(Verb::Go(Dir::North)));
        assert_eq!(verb("zas"), Some(Verb::Shoot));
        assert_eq!(verb("zapad"), Some(Verb::Go(Dir::West)));
        assert_eq!(verb("dolů"), Some(Verb::Down));
        assert_eq!(verb(""), None);
        assert_eq!(verb("xyz"), None);
    }

    #[test]
    fn object_is_rest_of_line() {
        let cmd = parse("vezmi  pytlík zlaťáků").unwrap();
        assert_eq!(cmd.object.as_deref(), Some("PYTLIK ZLATAKU"));
        assert_eq!(parse("vez").unwrap().object, None);
    }

    #[test]
    fn items_by_any_word() {
        assert_eq!(find_item("diamant"), Some(Item::Diamond));
        assert_eq!(find_item("lžičku"), Some(Item::Spoon));
        assert_eq!(find_item("plat"), Some(Item::Platinum));
        assert_eq!(find_item("rozsvícenou"), Some(Item::LitLamp));
        assert_eq!(find_item("svítilnu"), Some(Item::Lamp));
        assert_eq!(find_item("nic"), None);
    }

    #[test]
    fn collisions_resolve_like_original_then_by_full_word() {
        // V originálu "STR" znamenalo vždy lžičku, na straku nešlo odkázat.
        assert_eq!(find_item("str"), Some(Item::Spoon));
        assert_eq!(find_item("straku"), Some(Item::Magpie));
        assert_eq!(find_item("kleště"), Some(Item::Cutters));
    }
}
