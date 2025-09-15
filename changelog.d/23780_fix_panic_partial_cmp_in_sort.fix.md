Fix a bug where some float values like `f64::NAN`, `f64::INFINITY` and similars would cause Vector
to panic if the amount of items being sorted in those internal functions exceeded 20 items.

authors: thomasqueirozb
