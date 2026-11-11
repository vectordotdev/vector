Messages that are not valid UTF-8 no longer fail `octet_counting`, the framing the `syslog` source uses, in either its octet-counted or newline-delimited mode. UTF-8 handling is now done by the configured deserializer. `syslog` decoding replaces invalid sequences with U+FFFD by default (`lossy`). Because framing no longer errors, the `syslog` source no longer closes a connection that carried a non UTF-8 message. Previously the framing error ended the connection, losing every message after it on that connection.

authors: yackushevas
