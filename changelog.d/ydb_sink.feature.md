A new `ydb` sink has been added to deliver log and trace data to YDB (Yandex Database).

The sink selects between high-performance `bulk_upsert` and transactional `UPSERT` based on the table's schema
secondary indexes, and automatically refreshes the schema when table structure changes are detected.

authors: nepunep
