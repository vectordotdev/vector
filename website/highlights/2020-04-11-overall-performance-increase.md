---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "~36% Higher Throughput"
description: "We identified optimizations that net ~36% higher throughput"
author_github: "https://github.com/binarylogic"
pr_numbers: [2295, 2296]
release: "nightly"
hide_on_release_notes: false
tags: ["type: performance"]
---

For the past 3 releases we've been heads down trying to reach
[1.0][urls.vector_roadmap], but during this release we spent time explicitly
profiling Vector, looking for simple performance improvements. We were able to
identify 2 meaningful performance improvements:

1. [PR#2295][urls.pr_2295] swaps the types of our internal data-model from atoms to strings. (+~8%)
2. [PR#2296][urls.pr_2296] optimzes our data model value insertion and retrieval. (+~28%)

We're not done! We plan to follow up our all of our performance improvements
with a blog post detailing the process and findings.


[urls.pr_2295]: https://github.com/timberio/vector/pull/2295
[urls.pr_2296]: https://github.com/timberio/vector/pull/2296
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
