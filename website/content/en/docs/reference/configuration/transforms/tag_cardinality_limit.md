---
title: Tag cardinality limit
kind: transform
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Failed parsing

This transform stores, in memory, a copy of the key for every tag on every metric event seen by this transform. In `exact` mode, a copy of every distinct value for each key is also kept in memory, until `value_limit` distinct values have been seen for a given key, at which point new values for that key are rejected. To estimate the memory usage of this transform in mode exact you can use the following formula:

```
(number of distinct field names in the tags for your metrics * average length of
the field names for the tags) + (number of distinct field names in the tags of
your metrics * [`value_limit`](#value_limit) * average length of the values of tags for your
metrics)
```

In `probabilistic` mode, rather than storing all values seen for each key, each distinct key has a Bloom filter which can probabilistically determine whether a given value has been seen for that key. The formula for estimating memory usage in `probabilistic` mode is:

```
(number of distinct field names in the tags for your metrics * average length of
the field names for the tags) + (number of distinct field names in the tags of
-your metrics * cache_size_per_tag)
```

The [`cache_size_per_tag`](#cache_size_per_tag) option controls the size of the Bloom filter used for storing the set of acceptable values for any single key. The larger the Bloom filter the lower the false positive rate, which in our case means the less likely we are to allow a new tag value that would otherwise violate a configured limit. If you want to know the exact false positive rate for a given `cache_size_per_tag` and [`value_limit`](#value_limit), there are many free on-line Bloom filter calculators that can answer this. The formula is generally presented in terms of `n`, `p`, `k`, and `m`, where `n` is the number of items in the filter (`value_limit` in our case), `p` is the probability of false positives (what we want to solve for), `k` is the number of hash functions used internally, and `m` is the number of bits in the Bloom filter. You should be able to provide values for just `n` and `m` and get back the value for `p` with an optimal `k` selected for you. Remember when converting from `value_limit` to the `m` value to plug into the calculator that `value_limit` is in bytes, and `m` is often presented in bits (1/8 of a byte).

### Intended usage

This transform is intended to be used as a protection mechanism to prevent upstream mistakes, such as a developer accidentally adding a `request_id` tag. When this happens, we recommend fixing the upstream error as soon as possible. This is because Vector's cardinality cache is held in memory and is erased when Vector is restarted. This causes new tag values to pass through until the cardinality limit is reached again. For normal usage this shouldn't be a common problem since Vector processes are normally long lived.

### Restarts

This transform's cache is held in memory, and therefore, restarting Vector will reset the cache. This means that new values will be passed through until the cardinality limit is reached again. See [intended usage](#intended-usage) for more info.

### State

{{< snippet "stateless" >}}
