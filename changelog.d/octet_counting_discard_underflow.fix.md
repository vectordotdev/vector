Fixed an integer underflow in the octet-counting framer (used by TCP `syslog` sources) that occurred when an over-length, length-prefixed message was split across multiple reads. Previously the decoder could panic in debug builds, or in release builds wrap the remaining-bytes counter to a huge value, wedging the decoder and silently dropping all subsequent input on that connection.

authors: hhh6593
