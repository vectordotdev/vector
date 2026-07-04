Fixed the syslog codec silently ignoring short-form severity keywords (`crit`, `emerg`, `err`, `info`, `warn`) and falling back to the default `informational`. The encoder now accepts both short-form and full-form severity names, matching the values used by VRL's `to_syslog_severity` and `to_syslog_level` functions.

authors: vparfonov
