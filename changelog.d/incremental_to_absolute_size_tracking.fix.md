1. Fix memory tracking on MetricSet in incremental_to_absolute transform by accurately calculating metric sizes. Previously, all sizes were being calculated as 0, resulting in no actual tracking 
2. Add metrics for incremental_to_absolute to track events, internally tracked cache size, and evictions

authors: GreyLilac09
