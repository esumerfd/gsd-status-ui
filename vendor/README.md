# Vendored dependencies

## leaf

Vendored copy of [rivolink/leaf](https://github.com/rivolink/leaf) at upstream
`main`, plus two local patches not yet upstream:

- `feat: add embeddable lib target exposing markdown/theme as viewer facade`
- `chore: silence dead-code warnings in lib-only build`
- viewer facade: `searchable_lines` + `highlight_line` exposed for the
  adapter's in-document search (added after vendoring)

Last synced from local clone at commit `cabc4d825752225e525263850ffdb79a19b4cc21`.
Previously tracked as a git submodule pointing at a local path, which broke
clones and CI builds; the source is now committed directly (MIT licensed,
see `leaf/LICENSE`).
