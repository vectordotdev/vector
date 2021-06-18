#### Strings

Strings are UTF-8 compatible and are only bounded by the available system memory.

#### Integers

Integers are signed integers up to 64 bits.

#### Floats

Floats are 64-bit [IEEE 754][ieee_754] floats.

#### Booleans

Booleans represent binary true/false values.

#### Timestamps

Timestamps are represented as [`DateTime` Rust structs][date_time] stored as UTC.

##### Timestamp Coercion

There are cases where Vector interacts with formats that don't have a formal timestamp definition, such as JSON. In these cases, Vector ingests the timestamp in its primitive form (string or integer). You can then coerce the field into a timestamp using the coercer transform. If you're parsing this data out of a string, all Vector parser transforms include a `types` option, allowing you to extract and coerce in one step.

#### Time zones

If Vector receives a timestamp that doesn't contain timezone information, it assumes that the timestamp is in local time and converts the timestamp to UTC from the local time.

#### Null values

For compatibility with JSON log events, Vector also supports `null` values.

#### Maps

Maps are associative arrays mapping string fields to values of any type.

#### Arrays

Array fields are sequences of values of any type.

[date_time]: https://docs.rs/chrono/latest/chrono/struct.DateTime.html
[ieee_754]: https://en.wikipedia.org/wiki/IEEE_754
