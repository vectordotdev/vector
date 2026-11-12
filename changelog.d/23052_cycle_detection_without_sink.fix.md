Vector now rejects a configuration in which transforms form a cycle that does not feed any sink, reporting a "Cyclic dependency detected" error instead of overflowing its stack at startup. Previously such a cycle was only detected by `vector test`. The error now also lists only the components that form the cycle.

authors: anpawo
