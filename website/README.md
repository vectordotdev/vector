# vector.dev

This directory houses the assets used to build Vector's website and documentation, available at [vector.dev][vector].

## Prerequisites

* The [Hugo] static site generator. Make sure to install the extended version (with [Sass] and [ESBuild] support), specifically the version specified in [`netlify.toml`][netlify_toml].
* The [CUE] configuration and validation tool
* [Yarn]

## Run the site locally

```shell
make serve
```

## Tasks

### Add a new version of Vector

1. Add the new version to the `versions` list in [`cue/reference/versions.cue`][./cue/reference/versions.cue]

[cue]: https://cue-lang.org
[esbuild]: https://github.com/evanw/esbuild
[hugo]: https://gohugo.io
[netlify_toml]: ../netlify.toml
[sass]: https://sass-lang.com
[vector]: https://vector.dev
[yarn]: https://yarnpkg.com
