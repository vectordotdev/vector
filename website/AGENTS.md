# Vector Website Development

These instructions apply to changes under `website/`. See [README.md](README.md)
for the website architecture and prerequisites.

Target website changes to `master`.

Run the site locally from the repository root:

```bash
make generate-docs
cd website
make serve
```

The local site is available at <http://localhost:1313>.

Build CUE documentation from the `website/` directory:

```bash
make cue-build
```

After editing Markdown, run `make check-markdown` from the repository root.
