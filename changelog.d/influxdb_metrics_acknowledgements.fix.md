The `influxdb_metrics` sink now reports the result of each write to end-to-end acknowledgements. Before, metrics were acknowledged as delivered even when InfluxDB rejected the write or all retries failed, so disk buffers and sources with acknowledgements enabled could drop metrics that InfluxDB never stored. Rejected writes now mark their events as rejected, and writes that fail after retries mark them as errored.

authors: thomasqueirozb
