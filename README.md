# Poklad 2

Volná reimplementace textové adventury **Poklad 2** Františka Fuky
(FUXOFT, 1985, ZX Spectrum) v Rustu. Hraje se v terminálu.

Autorem původní hry, jejích textů, mapy bludiště, předmětů a hádanek je
**František Fuka**. Tahle verze je neoficiální a vznikla z lásky k originálu.

## O co jde

Bloudíš lesem a dvěma patry podzemí a hledáš pět pokladů. Cestu ti zkříží
had, lev, drak i zamčené dveře a ve tmě podzemí nepřežiješ bez svítilny.

Příběh, texty, mapa, předměty i hádanky odpovídají originálu, texty jsou
doslova převzaté z původního programu. Nové je ovládání a rozhraní:

- mapa prozkoumaných míst se zdmi a předměty, každé patro zvlášť
- pohyb klávesami (šipky nebo WSAD) i myší
- příkazy jde psát i naklikat, klik na předmět ho vezme nebo položí
- text se vypisuje jako na psacím stroji, poslední akce je barevně zvýrazněná
- při prvním vstupu do místnosti bublina s tím, co tu vidíš, a s radou
- přehled toho, co neseš, kolik hodin bloudíš a kolik máš pokladů

Dvě věci se od originálu liší i ve hře samotné: dveře se klíčem odemknou
natrvalo a rada (příkaz POMOC) zmizí, když překážku vyřešíš.

## Instalace na Linuxu

Stáhni soubor `poklad2-linux-x86_64` z
[posledního vydání](https://github.com/DoubyCz/poklad2/releases/latest),
povol spuštění a spusť ho:

```bash
chmod +x poklad2-linux-x86_64
./poklad2-linux-x86_64
```

Je to jeden statický soubor bez závislostí, poběží na libovolné distribuci.

Nejlépe vypadá v terminálu s podporou barev true color a myši (foot, kitty,
Alacritty, Ghostty, WezTerm, GNOME Terminal a další). Okno by mělo mít
aspoň 70 sloupců, jinak se nevejde postranní panel.

### Sestavení ze zdrojů

Potřebuješ Rust 1.88 nebo novější.

```bash
git clone https://github.com/DoubyCz/poklad2.git
cd poklad2
cargo run --release
```

Statický soubor (jako ve vydání) sestavíš přes musl:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Windows

Stáhni `poklad2.exe` z
[posledního vydání](https://github.com/DoubyCz/poklad2/releases/latest)
a spusť ho ve Windows Terminalu.

## Ovládání

Po úvodu je v hlavním okně mapa a text hry běží vpravo. Aktivní okno má bílý
rámeček.

| Klávesa | Co dělá |
|---|---|
| šipky, W S A D | sever, jih, západ, východ |
| R, F (PageUp, PageDown) | nahoru, dolů po patrech |
| Tab, Enter, klik do řádku | psaní příkazu |
| Esc, Tab v prázdném řádku | zpět k pohybu |
| ↑ ↓ při psaní | historie příkazů |
| Tab při psaní | doplnění příkazu nebo předmětu |
| Enter, mezerník, Esc | zavřít bublinu |
| Shift+PageUp, Shift+PageDown | rolování textu hry |
| Ctrl+D | konec |

Příkazy stačí zadávat prvními třemi písmeny, jako v originálu (`VEZ`, `POL`,
`ZAS`). Jde napsat i celý příkaz s předmětem na jeden řádek, třeba
`vezmi diamant`. Na diakritice nezáleží.

V mapě klik na místo, kudy vede cesta do jiného patra (`↓` `↑` `*`), ukáže
to patro. Levým tlačítkem dolů, pravým nahoru, Esc vrátí zpět.

Pozice se ukládají příkazem SAVE do `~/.local/share/poklad2/`
(na Windows do `%APPDATA%\poklad2\`) a nahrávají příkazem LOAD.

## Licence

Kód je pod licencí [MIT](LICENSE). Hra samotná (texty, mapa bludiště,
předměty a hádanky) je dílem Františka Fuky.
