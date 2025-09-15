Fixed a bug where certain floating-point values such as `f64::NAN`, `f64::INFINITY`, and similar would cause Vector to panic when sorting more than 20 items in some internal functions.

authors: thomasqueirozb
