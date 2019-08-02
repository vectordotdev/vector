---
description: Vector's deployment roles
---

# Roles

Once you have a general understanding of your [topology's][docs.topologies]
shape, you'll want to think about your deployment in terms of "roles".
Fortunately, Vector keeps this simple by serving just two roles:

{% page-ref page="agent.md" %}

{% page-ref page="service.md" %}

The agent role efficiently collects and forwards data while the service role
buffers, aggregates, and routes data. Both are described in more detail in
their respective documents.


[docs.topologies]: ../../../setup/deployment/topologies.md
