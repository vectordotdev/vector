---
title: Install Vector
sidebar_label: hidden
description: Install Vector on your platform
hide_pagination: true
---

Vector compiles to a single `musl` static binary with no dependencies, making it
simple to install:

import Tabs from '@theme/Tabs';

<Tabs
  defaultValue="humans"
  values={[
    { label: <><i className="feather icon-user-check"></i> For Humans</>, value: 'humans', },
    { label: <><i className="feather icon-cpu"></i> For Machines</>, value: 'machines', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="humans">

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
```

Enables prompts for a human to answer and confirm.

</TabItem>
<TabItem value="machines">

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh -s -- -y
```

Disables prompts and installs without input.

</TabItem>
</Tabs>

## Manual Installation

If you prefer a manual approach you can explore the various installation
methods below:

import Jump from '@site/src/components/Jump';

<Jump to="/docs/setup/installation/manual">Manual</Jump>
<Jump to="/docs/setup/installation/operating-systems">Operating systems</Jump>
<Jump to="/docs/setup/installation/package-managers">Package managers</Jump>
<Jump to="/docs/setup/installation/platforms">Platforms</Jump>



