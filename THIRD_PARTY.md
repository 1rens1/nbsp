# Third-party software

NBSP is MIT licensed. It bundles or links the following, under their own licenses.

- **pdfium** (BSD-3-Clause / Apache-2.0), shipped as `libpdfium` from
  [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries). Its license files come with that release.
- **Noto Sans** (SIL OFL 1.1), via `@fontsource-variable/noto-sans`.
- **Rust crates**: almost all MIT or Apache-2.0. A few are MPL-2.0 (`cssparser`, `selectors`, `dtoa-short`,
  `option-ext`, pulled in by Tauri); they are used unmodified.
- **JavaScript packages** (Svelte, SvelteKit, shadcn-svelte, bits-ui, Tailwind, Tauri API, Tabler icons...): MIT, ISC or
  Apache-2.0. `lightningcss` (MPL-2.0) is a build tool and isn't shipped.

NBSP calls these tools at runtime but doesn't include them: CUPS, curl, SANE (`scanimage`), gutenprint (`escputil`),
LibreOffice. They keep their own licenses.

Full per-crate and per-package licenses: `cargo metadata` in `src-tauri/`, and `node_modules/*/package.json`.
