The `databricks_zerobus` sink now supports Unity Catalog `VARIANT` columns. VARIANT columns are detected via the `arrow.parquet.variant` extension marker on the resolved Arrow schema, and their JSON values are encoded into the Parquet Variant binary representation before ingestion. VARIANT fields nested inside structs, lists, and maps are supported.

authors: flaviofcruz
