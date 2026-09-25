Added a `lossy` configuration option to the syslog source to handle messages containing invalid UTF-8 byte sequences. When enabled, invalid bytes are replaced with the Unicode replacement character (U+FFFD) instead of dropping the message.

authors: mezgerj
