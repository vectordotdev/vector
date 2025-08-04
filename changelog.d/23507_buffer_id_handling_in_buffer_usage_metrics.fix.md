This change improves how the `buffer_id` is used in buffer usage metrics. 
It ensures the `buffer_id` is properly owned and has the right lifetime to be safely included as a label in emitted metrics.

authors: vparfonov
