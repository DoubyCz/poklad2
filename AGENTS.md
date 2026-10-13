# z80-prepis-hry-do-rust

Experiment: přepis hry ze ZX Spectra (strojový kód Z80) do idiomatického Rustu
s pomocí AI.

## Motivace

Ověřit v praxi, jestli AI zvládne z binárky ze Spectra pochopit program
a přepsat ho do čitelného, idiomatického Rustu. Nejde o emulaci ani
o překlad instrukce po instrukci, ale o skutečný přepis logiky.

Otázky, na které má experiment odpovědět:

- Jak dobře AI rozumí disassembly Z80 bez původních zdrojáků?
- Kolik ruční práce je potřeba (mapa kód/data, pojmenování rutin)?
- Dá se výsledek ověřit proti originálu tak, aby byl přepis věrný?
- Je postup přenositelný na složitější hry (akční, s grafikou a zvukem)?

## Cíl

Přepsat do Rustu textovou adventuru **Rychlé šípy** od Františka Fuky (Fuxoft).

Proč textovka: minimum hardwarových triků, žádné časování ani sprity.
Logika je hlavně v datech (místnosti, předměty, slovník, hlášky) a v parseru.
Výstup v Rustu může být obyčejná terminálová aplikace.

## Změna cíle: Poklad 2 (2026-10-13)

Jirka nakonec zvolil **Poklad 2** (Fuxoft 1985, verze 2.1, "speech by VOICESOFT"),
taky textovku. Originál: `original/poklad2.tap` (z `~/Stažené/Poklad2.zip`).

**Zadání přepisu: celá hra v moderním Rustu.** Není to remaster ani věrná
kopie (per pixel, zvuk, řeč). **Příběh, dialogy, postavy, předměty a herní
mechaniky musí zůstat přesně jako v originálu, texty doslova.** Vylepšovat
se smí jen ovládání a UX (Jirka výslovně chtěl pár UX inovací).

Zjištěno z `.tap`:

- Bloky: BASIC zavaděč `POKLAD2` (autostart 10) → `pokl2scr` (SCREEN$) →
  `poklad2cd` CODE 24200, délka 41336 (do konce paměti).
- Zavaděč: `CLEAR 24199: LOAD ""SCREEN$: LOAD ""CODE: RANDOMIZE USR 24200`
  (POKE 23739 kolem LOADu jen potlačí výpis "Bytes:").
- Na 24200 je `JP 34019`, tam `LDIR` kopíruje 24203-34018 na 23552:
  obnoví systémové proměnné, kanály, **BASIC program** a proměnné a pokračuje
  v běhu. **Celá logika hry je v BASICu** (183 řádků, 24406-33987),
  disassembly Z80 skoro není potřeba.
- Výpis s diakritikou: `analyza/poklad2.bas`. Čísla v programu nemají
  skryté hodnoty odlišné od textu (ověřeno všech 407).
- Strojový kód: `USR 60000` = syntéza řeči (VOICESOFT, mluví `y$` přes `o$`),
  `USR 61826` = hák přes ERR_SP: BASIC chyby (kromě OK/STOP/EOF) přesměruje
  na `GO TO 514` (číslo řádku POKEuje ř. 0), tj. na ř. 1000. Řádek 34
  ("Co to má znamenat ?!") se běžným hraním nespustí.
  `USR 64036` volaný před SAVE/LOAD pozice. Font CHARS na 59136.
- Čeština: UDG 0x90-0xA4 = á ? č ď ě é ? ? í ? ? ž ? ň ó ů ý ř š ť ú
  (? = přemalovaná normální písmena, v textech se nepoužívají).
- Mapa: 60 místností, `x` 1-20 les, 21-40 první podzemí, 41-60 druhé.
  Sever x-4, jih x+4, západ x-1, východ x+1; zdi v `b$(x)` (4 bity,
  pořadí S,V,J,Z), dekódované z dlouhého řetězce `a$` na ř. 9000.

## Rust přepis (stav 2026-10-13: hotový, hratelný)

`cargo run --release`. Edition 2024, clippy `pedantic` čistý, jediná
závislost `ratatui` (s crosstermem, celoobrazovkové rozhraní s myší).

- `src/data.rs` - mapa (zdi jako bitové masky SVJZ), předměty, nápovědy
- `src/parser.rs` - příkazy podle prvních 3 písmen, hledání předmětů
- `src/game.rs` - stav a pravidla, bez I/O (vrací texty), testovatelné
- `src/ui/mod.rs` - konzole (tahá vstup přes `LineReader`, píše do `Write`),
  úvod, MAPA/NÁVOD, ukládání pozic; herní texty láme na 32 sloupců
- `src/ui/tui.rs` - celoobrazovkové rozhraní: `Screen` (výstup s barvami
  z ANSI a typewriter efektem), tlačítka na myš, řádek s historií a Tab
- `src/ui/input.rs` - `LineReader`, `PlainReader` (roura, testy), doplňování
- `src/ui/map.rs` - automapa prošlých míst, `src/ui/style.rs` - ANSI barvy

Rozhraní se zapne, když stdin i stdout jsou terminál, jinak jede obyčejný
řádkový režim. Testovat ho jde v `tmux -L poklad` (capture-pane = snímek
obrazovky jako text, kliknutí poslat `send-keys -l $'\e[<0;X;YM'` + `...m`).
- `tests/pruchod.txt` - kompletní průchod (108 tahů), test ho přehraje
  přes celé UI a čeká výhru

Kontrola textů: každý řetězec, který BASIC zobrazuje, musí být v `src/`
doslova (mezery slité). Skript na to byl ad hoc v Pythonu; při změnách textů
ho zopakovat (vytáhnout `"..."` z `analyza/poklad2.bas`, vynechat řeč `y$`,
`m$` a data `a$`). V přepisu jsou všechny texty, liší se jen zalomení
instrukcí (originál dělil slova na 32 sloupců: "počí-tači", "ne- platí").
Magnetofonové otázky (2500/2600) zůstaly, i když se ukládá do souboru.
Ř. 34 "Co to má znamenat ?!" se vypíše na Ctrl+C a hra skončí (originál
na BREAK resetoval počítač). A/N a "Stiskni libovolné tlačítko" berou
jeden stisk klávesy jako `INKEY$`.

Jirka si výslovně nepřeje ověřování proti běžícímu originálu (Fuse apod.),
hra je pochopitelná z kódu. Cíl: volná nová implementace, "jak by to Fuka
napsal dnes", ne kopie pixel po pixelu.

Řešení: ptáček u hada (3), klíč na dveře (5→1), svítilna ROZSVIT před
sestupem jámou (2), baterie (30) drží světlo napořád, jinak 100 tahů.
Puška+náboje: straka (27) → lžička, lev (33) → cesta na 29 a ke skafandru
(21). ALLAH ve 32 → les 20, studnou dolů se skafandrem → 60. Jed u draka (45)
→ platina (41). Hodinky 47. BALALAJKA v 53 je past (kobka 43).

**Vědomé odchylky v mechanice (Jirkovo rozhodnutí):**

- Odemčené dveře: kdo projde dveřmi (místnost 5) na sever s klíčem, ten je
  odemkne natrvalo (`Game::door_unlocked`). Pak se jmenují "odemčené dveře"
  (`game::UNLOCKED_DOOR`, jediný nový herní text), projde se i bez klíče
  a POMOC u nich radí obecné "Počínáš si dobře...". V originálu zůstávaly
  "zamčené" a klíč se kontroloval pokaždé. Uložená pozice má verzi 2
  (řádek `door`), verze 1 se dál načte.
- Rady POMOC zmizí, když je překážka vyřešená (`Game::hint`): jáma 2 s
  rozsvícenou svítilnou, had, dveře odemčené, studna se skafandrem, straka,
  lev, drak. Pak je obecná `data::DEFAULT_HINT`. Originál radil podle místnosti
  vždy. Rada se navíc ukazuje trvale v bloku "Kde jsi" pod popisem místa.
- Bublina u postavy (Jirkovo přání): jen při **prvním** vstupu do místnosti
  (konzole si pamatuje `greeted`), nikdy opakovaně. Obsah: co tu leží
  ("Vidím studnu.", "Vidím pušku a náboje.", názvy ze hry) a rada, pokud není
  `DEFAULT_HINT` (`ui::bubble_text`). Bez věcí i rady se neukáže. Bublina je
  v mapě nad @ (bez místa pod ním), text se vypíše psacím strojem (35 zn/s).
  Enter, mezerník nebo Esc ji dopíše a dalším stiskem zavře; zmizí i při
  další akci.
- Příkaz s předmětem jde napsat na jeden řádek (`vezmi diamant`), směry
  i celými slovy (`sever`, `dolů`), diakritika nevadí.
- Předmět jde označit kterýmkoliv slovem názvu (`lzicku`); v originálu jen
  prvním, takže na `STR` šla jen lžička a straka nešla pojmenovat.
- Klikání myší (Jirkovo přání): panel Kudy (směry), Příkazy (Vezmi…/Polož…
  jen napíšou sloveso, předmět se doklikne nebo dopíše), Vidíš/Neseš (klik
  = vezmi/polož, nebo doplní rozepsané sloveso). A/N mají tlačítka.
  Tajná slova ALLAH/BALALAJKA záměrně nemají tlačítko ani doplňování.
- Typewriter efekt (Jirkovo přání), klávesa nebo klik dopíše text hned.
- Rozložení (Jirkovo přání): úvod (titulek, instrukce) je v hlavní ploše,
  po "Stiskni libovolné tlačítko" převezme hlavní plochu mapa **jednoho
  patra** (Jirkovo přání: patra jako samostatné mapy, název jen v titulku).
  Ukazuje patro hráče (`Scale::Large`, vystředěná, kolečko roluje). Klik na
  místo spojení pater (prošlá jáma/studna/přenesení, `Map::connected_levels`,
  i na vlastní @) přepne na druhé patro: levým dolů, pravým nahoru. Esc nebo
  další pohyb vrátí na patro hráče. Klik jinam vystředí a text hry se vypisuje
  v pravém panelu (vnitřek 32 znaků jako Spectrum). Dole pod mapou legenda.
- Zdi (Jirkovo přání): při vstupu do místnosti se hned zakreslí všechny její
  zdi (`Map::record_wall` podle `data::neighbor`) jako `═`/`║` v barvě
  prostředí (`tui::wall_color`): les zeleně, 1. podzemí hnědě, 2. podzemí
  šedomodře (rozhraní ví, kde které patro v mapě začíná: `Map::level_columns`). Tlačítka
  Kudy směrem do zdi jsou šedá a neaktivní (vidět, ale neklikací); Nahoru/
  Dolů aktivní jen kde je jáma nebo studna.
  "Prozatím tam nemůžeš" (had, lev, drak, dveře) zeď není, tlačítko zůstává.
  Napsat jde dál jakýkoliv směr s původní odpovědí hry.
- Věci v mapě (Jirkovo přání): v navštívených místnostech (kromě té s @) se
  po sekundě střídají ikony věcí a značka jámy. Ikony jsou úzké znaky
  připomínající věc (`map::icon`, ověřeno, že je mají monospace fonty
  JetBrains/Caskaydia/Noto Sans Mono/DejaVu). Legendu ikon věcí v mapě
  Jirka nechce (pod mapou jen obecné značky); 1. pád (`map::label`) je jen
  ve výpisu příkazu MAPA. Emoji Jirka nechtěl (dva sloupce). Ikony i v panelech
  Vidíš/Neseš. Hlavní text hry ikony nemá.
  Cesty mezi patry: ↓ ↑ ↕ (jámy a studna vedou svisle na stejné místo),
  * = přenesení kouzelným slovem. Když hráč stojí na jámě, je značka
  v titulku mapy ("· tady ↑"), v buňce hráče je jen @.
- Ovládání klávesnicí (Jirkovo přání, RPG styl): aktivní je buď mapa, nebo
  řádek pro psaní (bílý rámeček, ostatní okna šedý). V mapě klávesy hýbou:
  šipky/WSAD = světové strany, R/F nebo PgUp/PgDn = nahoru/dolů, do zdi nic.
  Do řádku se přepne klikem, Tab nebo Enter (a sám u Co ?, jména pozice,
  po Vezmi…/Polož…); zpět Esc, Tab v prázdném řádku, Shift+Tab, odeslání
  příkazu nebo klik do mapy. V řádku šipky = historie, Tab = doplnění.
  Shift+PgUp/PgDn roluje text hry.
- Barvy textu podle stáří (Jirkovo přání): poslední příkaz hnědě, odpověď
  na něj žlutě, starší příkazy tmavě šedě, starší text šedě (`Screen::color`,
  hráčovo zadání značí `Screen::inputs`). Novou akci začíná herní příkaz
  nebo konec úvodu; odpovědi na Co ?, A/N a jména pozic patří k té předchozí.
- Šířka textu: řetězce zůstávají doslova pro 32 sloupců Spectra, ale před
  výpisem je `reflow` složí do vět (konec řádku Spectra = mezera, pomlčka na
  konci plného řádku = dělené slovo, zarovnávací mezery se slijí). Řádky do
  32 znaků zůstávají beze změny. Rozhraní pak láme po slovech na šířku okna.
- Pořadí pravého panelu (Jirkovo přání): text hry (bere volné místo),
  Kde jsi (s radou), Vidíš, Kudy, Příkazy, Neseš. Tlačítka Inventura, Mapa,
  Pomoc a Návod nejsou (stav, mapa i rada jsou vidět pořád), příkazy jdou
  napsat.
- Blok "Kde jsi" v panelu: popis místa, pod ním rada (POMOC), (`Turn::location`, totéž co
  v hlavním textu) a v dolním okraji hodiny a poklady (co jinak vypíše
  INVENTURA, bez spotřebované hodiny). Tlačítko Inventura proto není
  (Jirkovo přání), příkaz INVENTURA jde dál napsat.
- Šipky = historie (jen v rámci běhu), Tab doplňuje příkazy a předměty,
  které hráč vidí nebo nese. Kolečko a PageUp/PageDown rolují text.
- Barvy podle INK/PAPER originálu (vypne `NO_COLOR` nebo roura), posílané
  jako RGB barvy ZX Spectra (`style::Ink`), když `COLORTERM=truecolor`.
  **Důvod:** Jirkovo téma Omarchy je černobílé, celá paleta 16 barev ve
  foot.ini jsou šedé, takže ANSI 33 "žlutá" byla šedá. Bez truecolor se
  padá zpět na paletu. Tmux s `capture-pane -e` ukáže jen kód barvy, ne
  skutečný odstín v terminálu.
- MAPA: automapa jen prošlých míst a průchodů, nic neprozradí. NÁVOD (`?`)
  zopakuje instrukce. Obojí i prázdný řádek nezabere herní hodinu.
- Pozice: `~/.local/share/poklad2/<jméno>.sav`, LOAD vypíše uložené pozice.
- Ctrl+D / Ctrl+C ukončí hru. Bez řeči (VOICESOFT) a zvuků.
- Doplňky rozhraní jsou vypsané stylem `Style::Extra` (tlumeně), aby byly
  odlišitelné od textů originálu.

## Plánovaný postup (původní, pro Rychlé šípy)

1. **Získat binárku** (`.tap`/`.tzx`) z legálního zdroje a zjistit licenční
   stav (Fuka své staré hry dává volně ke stažení - ověřit).
2. **Zjistit, v čem je hra napsaná.** Může jít o BASIC, čistý assembler,
   nebo o hru postavenou na generátoru adventur (The Quill, PAW apod.).
   Pokud je to Quill/PAW, existují nástroje, které vytáhnou celou databázi
   hry přímo (`unquill`, `unPAWs`) a disassembly kódu enginu skoro není potřeba.
3. **Disassembly** přes SkoolKit: `tap2sna.py` → `sna2skool.py` → postupně
   doplňovat control file (mapa kód/data, pojmenování rutin).
4. **Analýza** - zmapovat datové struktury: místnosti, spojení, předměty,
   slovník, podmínky/akce, texty (pozor na možnou kompresi textu).
5. **Přepis do Rustu** - data jako Rust struktury (případně generované
   z extrahovaných dat), parser a herní smyčka ručně.
6. **Ověření** - stejné sekvence příkazů v emulátoru (Fuse) a v Rust verzi,
   porovnat výstup. Ideálně projít celou hru podle návodu.

## Počáteční poznatky

- **AI nečte binárku přímo.** Dekódování hex dumpu je chybové, disassembly
  musí dělat nástroj (SkoolKit, z80dasm). AI pak pracuje s assemblerem.
- **48K program se celý vejde do kontextu**, AI může vidět celý kód najednou.
- **Hlavní ruční práce je oddělení kódu od dat** - disassembler sám nepozná,
  co jsou instrukce a co texty/tabulky.
- **Ověření proti originálu je nutné**, bez něj je přepis jen odhad.
- **Čeština:** hra skoro jistě používá vlastní znakovou sadu s diakritikou
  (předefinované UDG nebo celý font). Bude potřeba zjistit mapování kódů
  znaků na Unicode.

## Prostředí (Jirkův notebook, ověřeno 2026-10-13)

- Rust: `rustc 1.98.1`, `cargo` - nainstalováno
- Emulátor: `fuse-emulator 1.9.1` (binárka `/usr/bin/fuse`) - nainstalováno
- SkoolKit: **chybí**, v AUR jako `skoolkit` (9.3, označený jako zastaralý)
- Assembler (`sjasmplus`, `pasmo`): **chybí**, zatím není potřeba
  (zpětné sestavení není cíl, jen pomůcka pro ověření disassembly)

## Publikace

- GitHub: `DoubyCz/poklad2` (veřejné), větev `main`, README česky, kód MIT
  (`LICENSE` s poznámkou, že hra, texty, mapa bludiště, předměty a hádanky
  jsou dílem Františka Fuky).
- Do repa nejde `original/` (.tap) ani `analyza/` (výpis BASICu), jsou
  v `.gitignore`. AGENTS.md a CLAUDE.md v repu být můžou (Jirkovo rozhodnutí).
- Vydání: značka `vX.Y.Z` spustí `.github/workflows/release.yml`, které
  sestaví `poklad2-linux-x86_64` (statický, musl) a `poklad2.exe` (MSVC,
  statický CRT) a přiloží je k vydání. AppImage není potřeba, statický
  binár je jeden soubor. CI (`ci.yml`) na `main`: clippy `-D warnings` a testy.
- Ukládání na Windows: `%APPDATA%\poklad2` (`save_dir`, když chybí HOME).
