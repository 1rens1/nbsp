# NBSP — No Bull-Shit Print

Print utility for any CUPS printer and SANE scanner (Tauri 2 + SvelteKit + shadcn-svelte, pdfium in Rust).
It was built for my personal Epson L3210, the only printer it has been verified on: detection for other printers and
the duplex-printer path are tested against simulated queues only.

## Printers

Nothing to configure per model. NBSP asks CUPS:

- the driver's PPD gives paper sizes, the custom size range and the driver's own options (quality, paper type, ...);
- IPP gives the output tray (face up stacks get printed in reverse, so page 1 ends on top) and whether there's a duplexer
  (one job with `sides=two-sided-*`; without one, NBSP prints the fronts, asks you to flip the stack, then prints the backs).

What CUPS can't know, how a manually flipped stack has to go back in, is set once per printer in
Settings > Output with a 2-sheet test. Nozzle check and head cleaning are Epson only (gutenprint's `escputil`, USB).

## Install

Download a package from the Releases page (releases are pre-releases until more printers have been tried):

```sh
sudo dnf install ./NBSP-*.rpm     # Fedora, openSUSE...
sudo apt install ./NBSP_*.deb     # Debian, Ubuntu...
```

macOS: a `.dmg` is built in CI but nobody has run it on a Mac yet. Windows isn't supported (no CUPS).

This puts NBSP in the app menu, registers it for PDFs/images/documents ("Open with"), and adds
`nbsp` to your PATH with bash/zsh/fish completions. To build and install from a checkout instead:
`./scripts/install.sh` (RPM only; re-run to update).

### Tools NBSP needs

| Tool | For | Package |
|---|---|---|
| `lp`, `lpstat`, `lpoptions`, `cancel` | printing | `cups-client` (Debian: `cups-client`; macOS: built in) |
| `curl` | reading the printer's PPD and IPP attributes from cupsd | `curl` |
| `scanimage` | scanning (the Scan button is hidden without it) | `sane-backends` (Debian: `sane-utils`; macOS: `brew install sane-backends`) |
| `escputil` | Epson nozzle check and cleaning, Epson only | `gutenprint` |
| `soffice` | .docx, .xlsx, .pptx, .odt | `libreoffice` |
| `pkexec` | installing the virtual printer, Linux only | `polkit` |

If a required tool is missing, NBSP says which. Settings > General > Diagnostics copies the versions and what NBSP detected
about your printer, which is what to paste into a bug report.

## Command line

```sh
nbsp report.pdf scan.jpg              # open files
nbsp print report.pdf -N 2 -d long    # 2 pages per sheet, double-sided, print now
nbsp print notes.pdf -p 1-4 -q draft -g --review   # apply settings, check them first
nbsp print zine.pdf --booklet         # booklet (fold & staple)
nbsp print doc.pdf --page-numbers "Page {page} of {total}" --number-position bottom-right
nbsp scan -o page.pdf --dpi 300       # scan straight to a file, no window
nbsp scan                             # open the scan dialog
nbsp status                           # printer, queue and device status
nbsp --help / nbsp print --help
```

Options given to `nbsp print` apply to that print only; they never change your saved defaults.

## Development

```sh
bun install
./scripts/fetch-pdfium.sh   # puts libpdfium.so in src-tauri/lib (not committed)
bun run tauri dev
bun test                    # layout planner tests
```

Runtime tools: see the table under Install.

## Releasing

`.github/workflows/ci.yml` checks every push (svelte-check, bun test, clippy, cargo test). Pushing a tag like `v0.1.0`
runs `release.yml`, which builds the rpm/deb (and the experimental dmg) and attaches them to a draft pre-release.
Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` first.

## Layout

- `src-tauri/src/engine.rs` — single pdfium thread: loading, rendering, sheet composition (pages placed as vector XObjects).
- `src-tauri/src/cli.rs` — CLI definition (clap); `build.rs` generates completions from it. `launch.rs` turns GUI commands into a request for the frontend, `headless.rs` runs the terminal-only ones.
- `src-tauri/src/cups.rs`, `ipp.rs`, `scan.rs`, `maintenance.rs` — CUPS (PPD + IPP), scanner, and escputil wrappers.
- `src-tauri/src/thumbs.rs` — loopback HTTP server for page thumbnails.
- `src-tauri/resources/cups/` — virtual "NBSP" printer (CUPS backend, PPD, install script). Jobs land in `/var/spool/nbsp` and open in the app.
- `src/lib/layout.ts` — imposition planner (n-up, booklet, scaling, margins, manual duplex passes). Tests: `bun test`.
- `src/lib/state/` — job (pages editor + undo), settings (persisted), printer, output (print flow).

## License

MIT, see [LICENSE](LICENSE). Bundled and linked third-party software: [THIRD_PARTY.md](THIRD_PARTY.md).
