Added a new `parse_float` option to the JSON decoder. When set to `"decimal"`, floating-point
numbers are parsed as exact decimal values (up to 28-29 significant digits) instead of IEEE 754
`f64`, which can lose precision for very large or high-precision numbers.

Note: When using Vector-to-Vector communication (`vector` source/sink with the native codec),
all receiving instances must be upgraded to this version before enabling `parse_float: "decimal"`
on any sender. Older Vector versions will silently drop events containing decimal values.
