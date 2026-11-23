# Prompt pro Claude

Implementuj projekt **Launcher Tags Sync** v Rustu přímo v tomto adresáři. Nejprve přečti `CLAUDE.md` a soubory v `docs/`. Neprováděj další analýzu osobních uživatelských dat; v `fixtures/` máš syntetické vstupy a očekávané výstupy. Cílem první verze je **bezpečný jednosměrný převod tagů Kvaesitso do Nicknames Quick Search**, nikoli hned automatická obousměrná synchronizace.

Navrhni workspace se dvěma crates (`launcher-tags-core`, `launcher-tags-cli`), napiš testy před implementací, implementuj nástroj s `inspect`, `sync`, `--dry-run`, `--report`, chybovými hlášeními, bezpečným zápisem a dokumentací. Testuj i idempotenci a zachování každého nesouvisejícího nastavení. Prověř, že převede všechny relevantní stránky `customizations.*` a že neimportuje `shortcut://` tagy jako alias aplikace.

Nejprve dolož úspěšné testy na syntetických fixturách. Případné soukromé zálohy chci využít pro integrační ověření jen lokálně a bez jejich přidání do Git repozitáře. V0.2 obousměrné slučování navrhni až po stabilní v0.1, ideálně se stavem synchronizace a řešením konfliktů.

Na konci předej konkrétní příkazy pro build a použití na macOS arm64 a Linuxu, výsledky testů, známá omezení a seznam vytvořených souborů. Implementaci nepovažuj za dokončenou bez přísných testů zachování dat.
