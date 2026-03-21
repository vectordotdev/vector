Added `ratio_field` and `rate_field` options to the `sample` transform to support dynamic per-event sampling, while requiring static `rate` or `ratio` fallback configuration and disallowing `ratio_field` and `rate_field` together.

authors: jhammer
