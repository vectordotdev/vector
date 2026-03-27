Fixed utilization for task transforms to not account for time spent when downstream
is not polling. If the transform is frequently blocked on downstream components,
the reported utilization should be lower.

authors: gwenaskell
