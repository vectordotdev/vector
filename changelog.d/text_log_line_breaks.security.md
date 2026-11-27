Vector's internal logs in the default text format now write carriage returns and line feeds inside log messages and field values as `\r` and `\n`. Previously, a value received from an external system, such as an error message returned by a server, could contain line breaks that ended the current log line and forged additional log lines. Messages that intentionally span several lines are now written on one line. The JSON log format already escaped these characters and is unchanged.

authors: bruceg
