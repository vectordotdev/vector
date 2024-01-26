---
title: Introducing Vector
description: "Hello World. Bringing Vector to life."
date: "2020-07-13"
authors: ["lukesteensen"]
badges:
  type: announcement
tags: ["vector", "timber", "observability"]
---

Today we're very excited to open source the Vector project! Vector is a tool for building flexible and robust pipelines for your logs and metrics data. We're still in the early stages, but our goal with Vector is to dramatically simplify your observability infrastructure while making it easy to get more value from your data.

<!--more-->

## Why build Vector?

We decided to build Vector because we believe that **existing tools take too narrow a view of the problem of building observability infrastructure**. Whether they're limited to working with a single storage system, not performant enough to handle your volume of data, or simply not flexible enough to let you do all the things you want with your data, the tools available today inevitably end up as a small part of a larger pipeline that your team must build and maintain.

Assembling and maintaining these pipelines piecemeal can be a tough and unrewarding job. There's a dizzying array of tools that all seem to cover unique subsets of the overall capabilities you're looking to implement and there's no guarantee that any given two of them will work well together. Faced with this, teams tend towards one (or a combination!) of three rough directions: (1) vendor-first, (2) trusted open source stacks, and (3) build your own.

**Vendor-first** is tempting, but usually expensive and invasive. The vendor provides their custom collectors and client libraries for you to integrate, and you simply ship them all your data. In the end, you're limited to their capabilities, locked into their ecosystem, and paying more and more every month as your data volume grows.

**Trusted open source stacks** give you a bit more flexibility, but with a significantly higher maintenance burden. You have to pick the right implementation of each sub-component (eight years ago there were already at least [20 implementations of statsd][statsd]), ensure they'll work together well, and then maintain and scale them for the lifetime of the infrastructure. Adding new capabilities or changing storage backends generally means adding a whole new set of components to run, often overlapping with some you're already running.

Finally, there are teams with the resources and organizational willpower to **build their own solutions**. This gives you an enormous amount of flexibility and power, but with an equally impressive price tag in engineering time.

## What is Vector?

So what exactly is Vector and how is it a better solution to this problem? At first glance, Vector looks a lot like some other logging infrastructure tools you might be familiar with. It can ingest data in a number of ways (tailing files, syslog messages over the network, etc), it can process that data in flight (regex and JSON parsing, filtering, sampling, etc), and it can send the raw or processed data to a variety of external systems for storage and querying. Whether it's rsyslog, logstash, fluentd, or another example, it's more likely than not that something with this basic set of features is running in your infrastructure.

On top of this basic functionality, Vector adds a few important enhancements:

1. A **richer data model**, supporting not only logs but aggregated metrics, fully structured events, etc
2. **Programmable transforms** written in lua that let you parse, filter, aggregate, and otherwise manipulate your data in arbitrary ways
3. **Uncompromising performance and efficiency** that enables a huge variety of deployment strategies

The end result is a tool that can **be** your pipeline, rather than just another component in it. We hope to enable the full flexibility and power of a custom built solution with a tiny fraction of the required investment. There have been a few other projects aimed in this direction (heka and cernan are two big inspirations), but with Vector we think we can take the ease of use and breadth of capabilities to another level.

## Try it out!

We're still very early in the process of building Vector, but it's already at a stage where we believe it's competitive with existing tools and ready to be trialled for production use. Not every feature mentioned above is present yet, but we're happy with the foundation and working quickly to flesh everything out. If you're interested in giving it a try, check out the project on GitHub, our docs, and please reach out! We'd love to hear about your use cases and are excited to see where we can take this project together.

[statsd]: https://joemiller.me/2011/09/list-of-statsd-server-implementations
