Fixed a hard-to-trigger race between closing a memory buffer and outstanding
sends that could rarely cause a lost event array at shutdown.

authors: bruceg
