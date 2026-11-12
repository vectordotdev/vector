On Linux, the `socket` and `syslog` sources now batch UDP and Unix datagram receives, improving ingestion throughput and reducing system call overhead under load. No configuration changes are required, and behavior on other platforms is unchanged.

authors: splitice
