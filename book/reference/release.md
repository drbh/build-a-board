# releasing a board

how a routed board becomes an order: the approved parts list, the review lock, and what `check`, `export` and `prep` produce. [examples/balance-bot](../../examples/balance-bot) is the example that goes all the way. [back to the book](../README.md)

## the approved parts list

`bom/approved-parts.csv` (the `bom` field of `Project`) is one line per part you have decided to buy. its columns:

```
refs,qty_per_board,order_spares,description,manufacturer,mpn,mouser_pn,digikey_pn,footprint,assembly,lifecycle,source_checked,notes
```

`refs` lists the references the line covers, separated by spaces. every line needs a `manufacturer`, `mpn`, a `lifecycle` of `Active`, a `source_checked` date, and a way to buy it: a `mouser_pn` or `digikey_pn`, or `notes` naming an Amazon ASIN or a JLCPCB/LCSC part. `assembly` is `SMT` for the lines that go on the JLCPCB assembly BOM; `Project::lcsc` maps those `mpn`s to LCSC numbers. every reference placed on the board must appear on some line.

## the review lock

```bash
bb lock-board
```

records the commit, the date, and the SHA-256 of the committed `.kicad_pcb` and `.kicad_sch` in `board-revision.json`, with the project's `decisions`. it refuses to run until both files are committed. every release command checks the lock first and stops if the board on disk, or the one in the reviewed commit, no longer matches: a changed board is reviewed, committed and locked again before it is released.

for this to work the KiCad project has to be in the repository, so a board meant for release sets `Project::new(..).kicad_dir(".")` and commits the routed board beside its sources. `bb` never implicitly regenerates a committed board it has no build record of.

## check

```bash
bb check
```

every gate, in order; the first failure stops it.

1. the lock matches the board
2. the approved parts list is complete and every line is sourced
3. the PCB netlist (`output/netlists/`) and the schematic netlist (`output/reports/`) connect every pad to the same net
4. KiCad's DRC and ERC report no errors (`output/reports/<name>-drc.rpt`, `-erc.rpt`)

then it writes the BOMs and the pick-and-place files, so you can read them before exporting.

## export and prep

```bash
bb export    # fabrication files and the JLCPCB upload set
bb prep      # the complete release
```

`export` runs the lock and BOM gates, writes the netlist, Gerbers and drills, the position and CPL files, and the JLCPCB set. `prep` runs every gate in `check` as well, renders every view and the schematic, and also writes the complete zip.

| path | what it is |
| --- | --- |
| `output/bom/<name>-bom.csv` | the approved list, as validated |
| `output/bom/<name>-jlcpcb-bom.csv` | the SMT lines with their LCSC numbers, in JLCPCB's format |
| `output/digikey-shopping-list.csv` | one line per part, quantities multiplied by `order_quantity` |
| `output/assembly/<name>-position.csv`, `-cpl.csv` | KiCad's positions, and the JLCPCB pick-and-place file with each part's `assembly_rotation` applied |
| `output/fabrication/` | Gerbers for the project's layers, Excellon drills and the drill map |
| `output/release/<name>-jlcpcb-gerbers.zip` | the fabrication directory, zipped |
| `output/jlcpcb/` | the Gerber zip, the JLCPCB BOM and the CPL side by side: what the upload form asks for |
| `output/release/<name>-complete.zip` | `prep` only: the board, schematic and project files, every file above, the renders, the reports, and the project's `release_files` |

release files are written deterministically: the same board produces byte-identical BOMs, CPL, netlist, lock and zips, and files written by `kicad-cli` differ only in the timestamps KiCad embeds.
