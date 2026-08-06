Fixed the VRL Playground truncating large integers (such as `xxhash` and `seahash` results) to their least-significant digits. Results are now serialized with full precision instead of being coerced to JavaScript floating-point numbers.

authors: stigglor
