Add an `on_conflict` option to the `postgres` sink. Setting this option to `do_nothing` appends `ON CONFLICT DO NOTHING` to the insert, so rows that violate a unique constraint are skipped instead of failing the whole batch. Skipped rows are reported as intentionally discarded events. The default, `error`, keeps the existing behavior.

authors: vaughnw128
