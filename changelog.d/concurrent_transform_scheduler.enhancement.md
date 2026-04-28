Improved CPU utilization for concurrent stateless transforms in certain scenarios (in particular when processing latency for event batches is both high and variable). The scheduler now lets batches complete in any order internally while preserving output ordering downstream, so new batches can keep being scheduled while a slow batch finishes, in order to optimally use available CPU resources.

authors: ArunPiduguDD
