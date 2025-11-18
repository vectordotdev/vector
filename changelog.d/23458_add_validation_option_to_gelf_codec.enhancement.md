The GELF decoder now supports a `validation` option with two modes: `strict` (default) and `relaxed`. When set to `relaxed`, the decoder will accept:

- GELF versions other than 1.1
- Additional fields without underscore prefixes
- Additional field names with special characters
- Additional field values of any type (not just strings/numbers)

This allows Vector to parse GELF messages from sources that don't strictly follow the GELF specification.

authors: ds-hystax
