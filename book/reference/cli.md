# the command line

every `bb` command and flag, what each writes, and the tools it calls. [back to the book](../README.md)

`bb [--board DIR] [COMMAND]`. `--board` is a board directory (a `bb-sdk` crate, or a directory with a prebuilt `board.wasm`) or a `.wasm` component; it defaults to the current directory, or `BB_BOARD`. with no command, `bb` runs `build`.

every command starts by building the crate (`cargo build --release --target wasm32-wasip2`, incremental, so an edit takes well under a second), running it in the sandbox, and resolving the design. problems in the design are reported here, before any stage runs.

## build

generate, route, draw the schematic and render, skipping each stage whose inputs have not changed. the flags work with or without the word `build`.

| flag | what it does |
| --- | --- |
| `--no-route` | stop at the unrouted board |
| `--no-schematic` | skip the connectivity schematic |
| `--no-render` | skip the images |
| `--quick` | render only the top and bottom views, not the perspective sheets and GLB model |
| `--force` | redo every enabled stage, even if nothing changed |
| `--attempts N` | FreeRouting runs before giving up (default 8, or `FREEROUTING_ATTEMPTS`) |
| `--timeout SECONDS` | seconds before a FreeRouting run counts as stalled (default 900, or `FREEROUTING_TIMEOUT_SECONDS`) |

the stages, and when each reruns:

1. **generate**, when the board component, the footprints it places, its artwork, or the `bb` or KiCad version change. the unrouted board is kept in `output/router/<name>-unrouted.kicad_pcb`
2. **route**, when the unrouted board changes. FreeRouting is retried until KiCad's DRC reports no unconnected items and no errors; each attempt's report is `output/router/route-attempt-N-drc.json`
3. **schematic**, when the board's connectivity changes. an existing schematic that already matches net for net is kept
4. **render**, when the board or schematic changes, or an image is missing

the record is `<name>.bb.json`, beside the KiCad project. a committed board that has no record (one reviewed in a repository that keeps its KiCad project beside the sources) is never rebuilt implicitly; `bb` asks for `--force` rather than discard its routing.

## preview

draw the board straight from its design, without KiCad, to `output/preview.html`: footprints, tracks, vias, zone outlines, text, artwork, and a ratsnest of what is still to route. text uses a stand-in font; it is a viewing aid, not an output.

| flag | what it does |
| --- | --- |
| `--watch` | rebuild on every save and serve a page that reloads itself, keeping pan and zoom and highlighting what each edit changed |
| `--open` | open it in the browser |
| `--port N` | port for `--watch` (default 7777; the next free one if taken) |
| `--routed` | draw copper from the generated or routed KiCad board instead, with KiCad's zone fills, so the ratsnest shows what routing left unconnected |

click a pad or track to highlight its net.

## doctor

list the external tools and where `bb` found them, then run `kicad-cli version` and `java -version`. fails if any is missing.

## schematic

rewrite the connectivity schematic and render it to SVG and PNG unconditionally. `--output DIR` puts the images somewhere other than `output/render/<name>/`.

## check, export, prep, lock-board

the release commands; [releasing a board](release.md) describes them and their files.

| command | what it does |
| --- | --- |
| `lock-board` | pin the committed PCB and schematic as the reviewed revision, in `board-revision.json` |
| `check` | every gate: lock, approved BOM, PCB/schematic connectivity, DRC, ERC; then the BOMs and the pick-and-place file |
| `export` | the fabrication files and the JLCPCB upload set, after the lock and BOM gates |
| `prep` | the complete release: every gate, every file, every render, both zips |

## clean

remove `output/`, keeping the KiCad project (it may hold a routed board, which cannot be regenerated identically). `--all` removes the KiCad project too. files tracked by git are always kept.

## tools

`bb` calls these programs and looks for them in this order: the environment variable, then `PATH`, then the platform default.

| tool | variable | default |
| --- | --- | --- |
| `kicad-cli` (fill, DRC, ERC, Gerbers, renders) | `KICAD_CLI` | `/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli` |
| KiCad's footprint library | `KICAD_FOOTPRINT_DIR` (or `KICAD10_FOOTPRINT_DIR`) | `/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints` |
| Java, for FreeRouting | `JAVA` | the Homebrew openjdk, then `java` |
| the FreeRouting jar | `FREEROUTING_JAR` | `~/.local/share/freerouting/freerouting-2.0.1.jar` |
| ImageMagick | `MAGICK` | `magick` |

building a board also needs `cargo` with the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`). `nix develop` sets all of this up, with KiCad from nixpkgs on Linux; on macOS it uses `/Applications/KiCad`.

## the output directory

everything `bb` writes goes under the board's `output/` (the KiCad project goes beside the sources instead when the board sets `Project::kicad_dir(".")`).

| path | written by |
| --- | --- |
| `<name>.kicad_pcb`, `.kicad_pro`, `.kicad_sch`, `<name>.bb.json` | `build` |
| `preview.html` | `preview` |
| `router/` | `build`: the unrouted board, the Specctra DSN and SES, one DRC report per attempt |
| `render/<name>/` | `build`, `schematic`, `prep`: `-wiring-top.png`, `-wiring-bottom.png`, `-3-view.png`, `-9-angle.png`, `.glb`, `.svg` and `-schematic.png` |
| `netlists/`, `reports/` | `check`, `export`, `prep`: the PCB netlist, the schematic netlist, DRC and ERC reports |
| `bom/`, `assembly/`, `fabrication/` | `check`, `export`, `prep`: BOMs, position and CPL files, Gerbers and drills |
| `jlcpcb/`, `release/`, `digikey-shopping-list.csv` | `export`, `prep`: the upload set and the zips |
