fix panics in disk buffer when both reader and writer are on the at last data file and last data file is corrupted.
which are generally the case when node shutdown improperly.

authors: anil-db
