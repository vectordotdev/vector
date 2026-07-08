The `clickhouse` sink's `arrow_stream` batch encoding now serializes events directly into the Arrow record batch instead of first materializing each event as an intermediate JSON value. This removes one allocation and one serialization pass per event on the encode hot path, reducing CPU by roughly 30% on wide or deeply nested events. Encoding behavior is otherwise unchanged, with one exception: non-finite floats (`inf`/`-inf`) are now passed through to the destination, whereas previously they were encoded as null.

authors: benjamin-awd
