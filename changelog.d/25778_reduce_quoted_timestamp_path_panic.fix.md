Fixed a `reduce` transform bug where a timestamp field with a name that requires quoting in a VRL path (e.g. `"created.at"` or `"event-time"`) would have its `_end` companion silently dropped from the reduced event. The companion path is now built structurally and correctly lands next to the base field.

authors: pront
