Added a new config field `missing_field_as` to the `databend` sink to specify the behavior when fields are missing. Previously the behavior was the same as setting this new configuration option to `ERROR`. The new default value is `NULL`.

authors: everpcpc
