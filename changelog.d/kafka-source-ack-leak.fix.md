The kafka source main loop has been biased to handle acknowledgements before new
messages to avoid a memory leak under high load.
