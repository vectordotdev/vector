This change introduces a new configuration option in the StatsD source: `convert_to` of type `ConversionUnit`. By default, timing values in milliseconds (`ms`) are converted to seconds (`s`). Users can set `convert_to` to `"milliseconds"` to preserve the original millisecond values.

authors: devkoriel
