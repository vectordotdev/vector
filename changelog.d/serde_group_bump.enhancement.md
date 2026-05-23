Bumped `serde_json` to `1.0.149` and `serde_with` to `3.18.0`. `serde_json` switched its float-to-string formatter from Ryū to Żmij in `1.0.147`, so floats serialized via the `native_json` codec may render with slightly different textual form (for example `1e+16` instead of `1e16`). The change is purely cosmetic: parsed `f32`/`f64` values round-trip identically, and Vector-to-Vector communication between old and new versions is unaffected.

authors: pront
