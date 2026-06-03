Fixed the Databricks Zerobus sink sending unresolved (relative) protobuf type
names to the Databricks ingestion server when schemas contain nested struct
fields. The server rejected such descriptors with
"Proto not self-contained for field '…' with type name '…'". The sink now
sends the pool-resolved descriptor with fully-qualified type names, matching
the behavior expected by SDK ≥ 2.0.1.

authors: gwenaskell
