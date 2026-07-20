Vector now parses configuration files before performing environment variable and `SECRET[...]` substitution.
Interpolation runs only on string-valued leaves in the parsed value tree, and a JSON-Schema-driven coercion
pass converts string scalars to the types declared by each component. This produces clearer error messages
with full field paths (e.g. `sources.my_source.count`) and makes the `--disable-env-var-interpolation` flag
behave consistently across the `vector`, `vector validate`, and `vector config` commands as well as the
HTTP provider.

authors: pront
