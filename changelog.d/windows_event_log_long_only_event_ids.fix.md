The `windows_event_log` source no longer fails to start when `only_event_ids` lists more than 23 IDs. The generated XPath query used a flat `or` chain, which Windows rejects with `ERROR_EVT_INVALID_QUERY` beyond 23 comparisons; lists longer than 20 IDs are now emitted as a balanced tree.

authors: pos-ei-don
