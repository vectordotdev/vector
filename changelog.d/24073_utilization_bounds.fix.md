Fixed an issue where utilization could report negative values. This could happen if messages from components were processed too late and were accounted for wrong utilization measurement period. These messages are now moved to the current utilization period, meaning there might be some inaccuracy in the resulting utilization metric, but it was never meant to be precise.

authors: esensar Quad9DNS
