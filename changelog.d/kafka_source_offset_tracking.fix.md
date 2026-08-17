Fixed the Kafka source to track contiguous offsets before committing back to Kafka. When end-to-end acknowledgements are enabled, the source now holds back the committed offset if an earlier message in the partition failed to deliver downstream. This prevents Kafka from skipping undelivered messages on consumer restart, providing correct at-least-once delivery semantics.

authors: rohitmanohar
