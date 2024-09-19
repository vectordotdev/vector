The `new_relic` sink, when sending to the `event` API, would quote field names
containing periods or other meta-characters. This would produce broken field
names in the New Relic interface, and so that quoting has been removed.
