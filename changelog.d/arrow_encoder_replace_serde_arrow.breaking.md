The `arrow_stream` codec now uses `arrow-json` instead of `serde_arrow` for Arrow encoding. This removes support for `DataType::Binary` columns. 

authors: benjamin-awd
