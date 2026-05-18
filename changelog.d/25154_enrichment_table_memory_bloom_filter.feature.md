Added bloom filter support for `memory` enrichment table, similar to cuckoo filter, providing as imple and efficient way to store and check presence of keys with a low memory footprint at the cost of false positives, but with less features that cuckoo filter.

authors: esensar Quad9DNS
