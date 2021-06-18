---
title: Log Event
description: Vector's internal log data model.
---

<SVG src="/optimized_svg/data-model-log_1526_907.svg" />

## Description

A Vector log event is a structured representation of a
point-in-time event. It contains an arbitrary set of
fields that describe the event.

A key tenet of Vector is to remain schema neutral. This
ensures that Vector can work with any schema, supporting
legacy and future schemas as your needs evolve. Vector
does not require any specific fields, and each component
will document the fields it provides.
