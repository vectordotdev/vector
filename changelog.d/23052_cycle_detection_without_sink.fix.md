Vector now rejects a configuration in which transforms form a cycle that does not feed any sink, reporting a "Cyclic dependency detected" error instead of overflowing its stack at startup. Previously such a cycle was only detected by `vector test`. Validation reports every cyclic group of components together, with names sorted for consistent diagnostics.

authors: anpawo pront
