# Vector's Documentation

[![Netlify Status](https://api.netlify.com/api/v1/badges/abeaffe6-d38a-4f03-8b6c-c6909e94918e/deploy-status)](https://app.netlify.com/sites/vector-project/deploys)

This directory houses the assets used to build Vector's website and documentation, available at [vector.dev][vector].

## Prerequisites

In order to run the site locally, you need to have these installed:

* The [Hugo] static site generator. Make sure to install the extended version (with [Sass] and [ESBuild] support), specifically the version specified in [`netlify.toml`][netlify_toml].
* The [CUE] configuration and validation language. See the value of `CUE_VERSION` in [`amplify.yml`](./amplify.yml) to see which version of CUE is currently being used for the docs.
* The [Yarn] package manager (used for static assets).
* [htmltest] for link checking.

## How it works

The Vector documentation is built on [Hugo], a static site generator with the following details:

* The [reference documentation] is powered by manually curated data located in the [`cue` directory](./cue).
* Other pages, such as the [guides], are powered by markdown files located in the [`content` directory](./content).
* Layouts and custom pages are powered by HTML files located in the [`layouts` directory](./layouts).
* Search is powered by Alogolia through a custom implementation.

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

## Redirects

Redirects for vector.dev are defined in three difference places (depending on the use):

1. Domain-level redirects, e.g. chat.vector.dev to our Discord server, are defined in [`netlify.toml`](../netlify.toml) in the repo root.
2. Splat-style redirects (which can't be defined as Hugo aliases) are defined in [`./static/_redirects`](./static/_redirects).
3. Redirects for specific pages are defined in the `aliases` field in that page's front matter.

## Link checker

vector.dev uses the [htmltest] link checker to sniff out broken links. Whenever the site is built in CI, be it the preview build or the production build, all internal links on the site are checked. If *any* link is broken, the build fails. If you push changes to the docs/site and your build yields a big red X but running the site locally works fine, scan the CI output for broken links. There's a good chance that that's your culprit.

You can run the full CI builds locally, with link checking included:

```shell
# Production
make local-production-build

# Preview
make local-preview-build
```

The standard link checking configuration is in [`.htmltest.yml`](./.htmltest.yml). As you can see from this config, external links are *not* checked (`CheckExternal: false`). That's because external link checking makes builds highly brittle, as they become dependent upon external systems, i.e. if CloudFlare has an outage or an external site is down, the vector.dev build fails. The trade-off here, of course, is that broken external links can go undetected. The semi-solution is to periodically run ad hoc external link checks:

```shell
make local-production-build
make run-external-link-checker
```

That second make command runs htmltest using the [`.htmltest.external.yml`](./htmltest.external.yml) configuration, which sets `CheckExternal` to `true`. We should strive to run this periodically in local environments to make sure we don't have too much drift over time.

## Known issues

* Tailwind's [typography] plugin is used to render text throughout the site. It's a decent library in general but is also rather buggy, with some rendering glitches in things like lists and tables that we've tried to compensate for in the `extend.typography` block in the [Tailwind config](./tailwind.config.js), but it will take some time to iron all of these issues out.

[cue]: https://cue-lang.org
[esbuild]: https://github.com/evanw/esbuild
[guides]: https://vector.dev/guides/
[htmltest]: https://github.com/wjdp/htmltest
[hugo]: https://gohugo.io
[netlify_toml]: ../netlify.toml
[reference documentation]: https://vector.dev/docs/reference/
[sass]: https://sass-lang.com
[tailwind]: https://tailwindcss.com
[typography]: https://github.com/tailwindlabs/tailwindcss-typography
[vector]: https://vector.dev
[yarn]: https://yarnpkg.com
