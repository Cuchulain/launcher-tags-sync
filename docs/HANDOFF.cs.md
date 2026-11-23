# Launcher Tags Sync — podklady pro implementaci v Rustu

Připravené zadání pro Claude (implementaci zatím **neobsahuje**). Převádí tagy aplikací z **Kvaesitso** na vyhledávací přezdívky (Nicknames) v **Quick Search** a později umožní bezpečné obousměrné slučování.

**Nejprve otevři `CLAUDE.md` a `docs/PLAN.md`.** Testovací vzorky jsou kompletně syntetické; soukromé zálohy uživatele nejsou v balíčku.

## Stav a ověřená funkčnost

V reálném jednorázovém převodu dne 23. 11. 2026 proběhl úspěšný import upravené zálohy do Quick Search. Referenční počty:

| Metrika | Hodnota |
|---|---:|
| Tagy aplikací Kvaesitso (řádky) | 521 |
| Unikátní balíčky aplikací s tagy | 325 |
| Nové Nicknames | 318 |
| Rozšířené původní Nicknames | 3 |
| Původní Nicknames beze změny | 4 |
| Původní Nicknames celkem | 7 |
| Přeskočené tagy zástupců aplikací | 8 |

Tyto hodnoty jsou cílem **volitelného soukromého integračního testu**, ne očekávaným výsledkem syntetických fixtures.

## Umístění do adresáře (historický návod z předávky)

Po stažení ZIPu rozbal takto (pokud není ZIP v `~/Downloads`, uprav cestu):

```shell
mkdir -p ~/Projects
unzip -n ~/Downloads/launcher-tags-sync-claude-kit.zip -d ~/Projects
```

Výsledný projekt bude v `~/Projects/launcher-tags-sync/`. Přepínač `-n` neumožní přepsání už existujících souborů.

## Předání Claude

1. Předej Claude celý tento adresář jako projekt/repozitář.
2. V úvodu konverzace použij `PROMPT_FOR_CLAUDE.md`.
3. Claude má nejprve implementovat testy na `fixtures/` a až pak Rust knihovnu a CLI.
4. Před importem upravené zálohy vždy exportuj aktuální Quick Search nastavení a ponech originály.

## Cílové použití (příkaz navržený ve specifikaci, dosud neimplementovaný)

```shell
launcher-tags-sync sync \
  --kvaesitso kvaesitso.kvaesitso \
  --quicksearch quick-search.quicksearch \
  --output merged.quicksearch \
  --report merged-report.json
```

Bezpečný náhled:

```shell
launcher-tags-sync sync --kvaesitso kvaesitso.kvaesitso \
  --quicksearch quick-search.quicksearch --dry-run
```

## Dokumentace

- `CLAUDE.md` — implementační kontrakt a bezpečnostní pravidla
- `docs/PLAN.md` — milníky implementace
- `docs/FORMATS.md` — ověřená struktura obou záloh
- `docs/SEMANTICS.md` — mapování, slučování, konflikty
- `docs/TESTS.md` — testovací matice a očekávání
- `docs/SECURITY.md` — soukromí a práce se zálohami
- `docs/SOURCES.md` — odkazy na relevantní soubory open-source projektů
- `fixtures/` — anonymní syntetické testovací zálohy a očekávané výsledky

**Licence projektu:** navrhuje se MIT nebo Apache-2.0; finální rozhodnutí před publikací. Zdrojové kódy aplikací nejsou součástí balíčku.
