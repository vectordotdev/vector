---
title: Enrichment tables
description: Enrich your observability events using static data from files
author_github: https://github.com/lucperkins
domain: config
weight: 5
---

Observability data **enrichment**

There are numerous ways that you can enrich observability events.

Enrichment tables are useful when your events carry data that can be correlated with other data.

## Supported formats

Currently, Vector supports comma-separated values ([CSV])

## Configuring enrichment tables {#configuring}

You can configure a Vector instance to use a file as an enrichment table

You can use as many files as enrichment tables as you like, provided that those tables are given
different names.

## Using enrichment tables in Vector {#using}

You can use [configured](#configuring) enrichment tables in Vector inside of [`remap`][remap]
transforms using [Vector Remap Language][VRL] (VRL), which provides two functions specific to
enrichment tables:

* [`find_enrichment_table_records`][find] searches an enrichment table for one or more rows that
  match the query condition(s). This function is [infallible] (it doesn't throw errors) because a
  query that matches no rows returns an empty array.
* [`get_enrichment_table_record`][get] search an enrichment table for exactly one row that matches
  the query condition(s). Unike `find_enrichment_table_records`, this function is fallible, as VRL
  returns an error if either no rows or multiple rows are found.

[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
[find]: /vrl/functions/#find_enrichment_table_records
[get]: /vrl/functions/#get_enrichment_table_record
[geoip]: /docs/reference/configuration/transforms/geoip
[infallible]: /vrl/errors/#runtime-errors
[remap]: /docs/reference/configuration/transforms/remap
[vrl]: /vrl
