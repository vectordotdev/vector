The `lua` transform no longer panics when a metric tag contains a value that cannot be converted to a string (e.g. a boolean or a nested table). Such values now produce a Lua conversion error and the event is discarded.

authors: quwin
