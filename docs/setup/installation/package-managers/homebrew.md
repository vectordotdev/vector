---
description: Install Vector through the Homebrew package manager
---

# Homebrew Package Manager

Vector can be installed through [Homebrew][url.homebrew] which is generally
used on MacOS systems.

## Install

Add the Timber tap and install `vector`:

```bash
brew tap timberio/brew && brew install vector
```

Start Vector:

```bash
brew services start vector
```

That's it! Proceed to [configure](#configuring) Vector for your use case.

## Configuring

The Vector configuration file is placed in:

```
/usr/local/etc/vector/vector.toml
```

A full spec is located at `/usr/local/etc/vector/vector.spec.toml` and examples
are located in `/usr/local/etc/vector/examples/*`. You can learn more about
configuring Vector in the [Configuration][docs.configuration] section.

## Administering

Vector can be managed through the [Homebrew services][url.homebrew_services]
manager:

{% page-ref page="../../../usage/administration" %}

## Uninstalling

```bash
brew remove vector
```

## Updating

```bash
brew update && brew upgrade vector
```


[docs.configuration]: ../../../usage/configuration/README.md
[url.homebrew]: https://brew.sh/
[url.homebrew_services]: https://github.com/Homebrew/homebrew-services
