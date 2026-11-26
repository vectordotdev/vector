The TCP-based sources (`socket` in `tcp` mode, `syslog`, `fluent`, `logstash`, and `statsd`) no longer slow down a busy connection when many other connections are open but idle. Before, idle connections took turns holding the source's shared read slots while they waited for data, so a busy connection could drop to less than 1% of its normal throughput. Idle connections now wait for data before they take a slot. The limit on events that a source holds in memory is unchanged.

authors: JonSnow1807
