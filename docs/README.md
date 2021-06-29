# vector.dev

This directory houses the assets used to build Vector's website and documentation, available at [vector.dev][vector].

## Prerequisites

In order to run the site locally, you need to have these installed:

* The [Hugo] static site generator. Make sure to install the extended version (with [Sass] and [ESBuild] support), specifically the version specified in [`netlify.toml`][netlify_toml].
* The [CUE] configuration and validation language. See the value of `CUE_VERSION` in [`amplify.yml`](./amplify.yml) to see which version of CUE is currently being used for the docs.
* The [Yarn] package manager (used for static assets).
* [htmltest] for link checking.

## Run the site locally

```shell
make serve
```

## Tasks

### Add a new version of Vector

1. Add the new version to the `versions` list in [`cue/reference/versions.cue`](./cue/reference/versions.cue). Make sure to preserve reverse ordering.
1. Generate a new CUE file for the release by running `make release-prepare` in the root directory of the Vector repo.
1. Add a new Markdown file to [`content/en/releases`](./content/en/releases), where the filename is `{version}.md` (e.g. `0.12.0.md`) and the file has metadata that looks like this:

    ```markdown
    ---
    title: Vector v0.13.0 release notes
    weight: 19
    ---
    ```

    The `title` should reflect the version, while the `weight` should be the weight of the next most recent version plus 1. The file for version 0.8.1, for example, has a weight of 8, which means the weight for version 0.8.2 (the next higher version) is 9. This metadata is necessary because Hugo can't sort semantic versions, so we need to make the ordering explicit. If Hugo ever does allow for semver sorting, we should remove the `weight`s.

## Known issues

* Tailwind's [typography] plugin is used to render text throughout the site. It's a decent library in general but is also rather buggy, with some rendering glitches in things like lists and tables that we've tried to compensate for in the `extend.typography` block in the [Tailwind config](./tailwind.config.js), but it will take some time to iron all of these issues out.

[cue]: https://cue-lang.org
[esbuild]: https://github.com/evanw/esbuild
[htmltest]: https://github.com/wjdp/htmltest
[hugo]: https://gohugo.io
[netlify_toml]: ../netlify.toml
[sass]: https://sass-lang.com
[tailwind]: https://tailwindcss.com
[typography]: https://github.com/tailwindlabs/tailwindcss-typography
[vector]: https://vector.dev
[yarn]: https://yarnpkg.com
