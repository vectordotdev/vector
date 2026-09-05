Prevent disk buffers from stalling when reader or writer progress occurs immediately before the
other side begins waiting. Newly written records are also held until the writer publishes the
corresponding buffer accounting.

authors: graphcareful
