---
title: The Vector API
short: API
weight: 3
---

Vector ships with a [GraphQL] API that allows you to interact with a running Vector instance. This page covers how to configure and enable Vector's API.

## Configuration

{{< api/config >}}

## Endpoints

{{< api/endpoints >}}

## How it works

### GraphQL

Vector chose [GraphQL] for its API because GraphQL is self-documenting and type safe. We believe that this offers a superior client experience and makes Vector richly programmable through its API.

### Playground

Vector's GraphQL API ships with a built-in playground that allows you to explore the available commands and manually run queries against the API. This can be accessed at the `/playground` path.

[graphql]: https://graphql.org
