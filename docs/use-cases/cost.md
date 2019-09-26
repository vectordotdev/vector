---
description: How Vector can dramatically reduce logs & metrics costs
---

# Reduce Cost

![][assets.reduce-cost]

One of Vector's primary uses cases is to reduce cost without disrupting workflows. This document will cover the various strategies to achieve this. The sections have intentional ordering, we recommend starting with the first section and working your way down until you achieve a satisfactory cost reduction.

## Transaction Sampling At The Source

The first cost reduction tactic we recommend is sampling transactions at the source. Notice we use the term "transactions". This is a very important distinction from random sampling, in that you are sampling _entire_ transactions related to your application. For example, if your application is processing web requests then an individual web request is a transaction.


[assets.reduce-cost]: ../assets/reduce-cost.svg
