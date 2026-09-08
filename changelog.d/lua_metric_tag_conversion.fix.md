Prevent the Lua transform from panicking when a metric tag contains a non-string value. Such values now produce a Lua conversion error and the event is discarded.

authors: quwin
