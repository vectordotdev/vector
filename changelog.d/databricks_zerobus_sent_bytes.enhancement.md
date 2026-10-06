The `databricks_zerobus` sink now reports `component_sent_bytes_total` (tagged `protocol: grpc`) for acknowledged batches, using the approximate wire size reported by the Zerobus SDK's Arrow Flight stream stats. The Zerobus SDK is upgraded to 2.11.0.

authors: flaviofcruz
