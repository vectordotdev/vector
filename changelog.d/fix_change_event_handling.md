When file change event for component changes and vector config change event comes very closely, config change event get discarded by component chnage. Fixing this issue by tracking and giving preference to reload from disk if there was event for that. 

authors: anil-db
