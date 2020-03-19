---
title: Install Vector via Homebrew
sidebar_label: Homebrew
description: Install Vector through the Homebrew package manager
---

Vector can be installed through [Homebrew][urls.homebrew] which is generally
used on MacOS systems.

## Install

<div className="steps steps--h3">

1. ### Add the Timber tap and install `vector`

   ```bash
   brew tap timberio/brew && brew install vector
   ```

2. ### Start Vector

   ```bash
   brew services start vector
   ```

   That's it! Proceed to [configure](#configuring) Vector for your use case.

</div>

### Previous Versions

Historical Vector versions can be found in the [releases][urls.vector_releases].
Once you've found the version you'd like to install you can specify it with:

```bash
brew install vector@X.X.X
```

## Configuring

The Vector configuration file is placed in:

```text
/usr/local/etc/vector/vector.toml
```

A full spec is located at `/usr/local/etc/vector/vector.spec.toml` and examples
are located in `/usr/local/etc/vector/examples/*`. You can learn more about
configuring Vector in the [Configuration][docs.configuration] section.

## Administering

Vector can be managed through the [Homebrew services][urls.homebrew_services]
manager:

import Jump from '@site/src/components/Jump';

<Jump to="/docs/administration">Administration</Jump>

## Uninstalling

```bash
brew remove vector
```

## Updating

```bash
brew update && brew upgrade vector
```


[docs.configuration]: /docs/setup/configuration/
[urls.homebrew]: https://brew.sh/
[urls.homebrew_services]: https://github.com/Homebrew/homebrew-services
[urls.vector_releases]: https://vector.dev/releases/latest
