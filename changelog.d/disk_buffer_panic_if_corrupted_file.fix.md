Fix disk buffer panics when both reader and writer are on the last data file and it is corrupted. This scenario typically occurs when a node shuts down improperly, leaving the final data file in a corrupted state.

authors: anil-db
