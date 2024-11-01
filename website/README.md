# The Vector website and documentation

[![Netlify Status](https://api.netlify.com/api/v1/badges/abeaffe6-d38a-4f03-8b6c-c6909e94918e/deploy-status)](https://app.netlify.com/sites/vector-project/deploys)

This directory houses all of the assets used to build Vector's website and documentation, available at [vector.dev][vector].

## Prerequisites

In order to run the site [locally](#run-the-site-locally), you need to have these installed:

* The [Hugo] static site generator. Make sure to install the extended version (with [Sass] and [ESBuild] support), specifically the version specified in [`netlify.toml`](../netlify.toml).
* The CLI tool for the [CUE] configuration and validation language.
* [Node.js] and the [Yarn] package manager (for static assets and some scripting).
* [htmltest] for link checking.

## How it works

vector.dev is a complex site with a lot of moving parts. This section breaks the site down into some key components.

### Netlify

vector.dev is built by and hosted on the [Netlify] platform. [Deploy previews] are built for *all* pull requests to the Vector project (though this may change in the near future).
 You can update site configuration and see the results of site builds on the Netlify [project page][netlify_project]. The configuration for Netlify is in the root of this repo, in the [`netlify.toml`][netlify_toml] file.

#### Branches

The current Vector release branch (`vX.X`, for example `v0.15`) branch is used to build the "production" site at https://vector.dev. All changes should be targeted to `master`. If you want to release a website change outside of the normal release cadence, you can cherry-pick the commit to the release branch.

The `master` branch, on the other hand, often contains unreleased, "nightly" changes to Vector that we don't yet want reflected on the website. The nightly version of the site is available at https://master.vector.dev. This version of the site may be useful for Vector users taking advantage of not-yet-released features.

### Static site generator

vector.dev is built using the [Hugo] static site generator. The site configuration is in [`config.toml`](./config.toml). The standard Hugo [directory structure] is obeyed.

### Cargo data

Some pages in the Vector documentation rely on dependency information such as version numbers found in the [`../Cargo.lock`](../Cargo.lock) file. Whenever you build the Vector site, the `../Cargo.lock` file is copied into `data/cargo-lock.toml` so it can be used in conjunction with Hugo's templating system to build HTML.

### Structured data

The Vector documentation relies heavily on structured data supplied using the [CUE] configuration and data validation language. Uses of CUE data include the docs for Vector's many [components] and the docs for [Vector Remap Language][vrl].

All of the CUE sources for the site are in the [`cue`](./cue) directory. Whenever you build the Vector site, the CUE sources are compiled into a single JSON file that's stored at `data/docs.json`. That information is then used in conjunction with Hugo's templating system to build HTML.

There's a variety of helper commands available for working with CUE. Run `make cue-help` for CLI docs.

> Having trouble with CUE? See [CUE pro tips](#cue-pro-tips) below for some pointers.

### JavaScript

For the most part, vector.dev uses the [Alpine] framework for interactive functionality. If you see directives like `x-show`, `x-data`, `@click`, and `:class` in HTML templates, those are Alpine directives. Alpine was chosen over jQuery and other frameworks for the sake of maintainability. Alpine directives live inside your HTML rather than in separate JavaScript files, which enables you to see how a component behaves without referring to an external `.js` file.

The [Spruce] library is used for all JavaScript state management. It stores things like light/dark mode preferences in `localStorage` and makes those values available in Alpine-wired components. See the [`app.js`](./assets/js/app.js) for managed state values.

The [Tocbot] library is used to auto-generate documentation table of contents on each page. The TOC is generated at page load time.

You'll also find two [React.js] components on the site: the spinning globe on the main page and the interactive search bar. The [TypeScript] for those components is in [`home.tsx`](./assets/js/home.tsx) and [`search.tsx`](./assets/js/search.tsx), respectively. React.js compilation is configured using the [`babel.config.js`](./babel.config.js) file and TypeScript compilation is configured using the [`tsconfig.json`](./tsconfig.json) file.

All JavaScript for the site is built using [Hugo Pipes] rather than tools like Webpack, Gulp, or Parcel.

### CSS

Most of the site's CSS is provided by [Tailwind], which is a framework based on CSS utility classes. The Tailwind configuration is in [`tailwind.config.js`](./tailwind.config.js); it mostly consists of default values but there are some custom colors, sizes, and other attributes provided there. Tailwind was chosen for the sake of maintainability; having most CSS *inside* the HTML templates makes it easier to understand and update a given component's styling. CSS post-processing for Tailwind is performed by [PostCSS], which is configured via the [`postcss.config.js`](./postcss.config.js) file.

In addition to Tailwind classes, some CSS is built from [Sass] (all Sass files are in [`assets/sass`](./assets/sass)):

* [`home.sass`](./assets/sass/home.sass) styles some elements that are only on the home page
* [`syntax.sass`](./assets/sass/syntax.sass) provides the colors for syntax highlighting
* [`toc.sass`](./assets/sass/toc.sass) styles documentation pages' table of contents. Tailwind doesn't work for this because the HTML for the TOCs is generated at page load time by [Tocbot].
* [`unpurged.sass`](./assets/sass/variables.sass) contains all the CSS that should *not* be run through PostCSS. The problem in some cases is that PostCSS [purges][purgecss] classes that aren't found in the HTML that's built by Hugo because they're built by other processes, like JavaScript that runs at load time. Anything in `unpurged.sass` escapes the purging process.

### Search

Search for vector.dev is provided by [Typesense]. Our search solution is largely custom:

* The [`typesense-index.ts`](./scripts/typesense-index.ts) script generates an index of all of the relevant pages on the site and stores the result in a single JSON file (output to `public/search.json`).
* The [`typesense-sync.ts`](./scripts/typesense-sync.ts) script syncs the generated JSON index with the Typesense backend, performing all the necessary create, update, and delete operations, using a custom package, `typesense-sync`. Reach out in #websites for more details.

The Typesense configuration for the site is captured via the [`typesense.config.json`](./typesense.config.json) file.


#### De-indexing pages

If you need to prevent a page from being indexed, you can add `noindex: true` to the page's metadata. Here's an
example:

```yaml
---
title: Don't index me, bro
noindex: true
---
```

### Icons

vector.dev uses two different icon sets for different purposes:

1. [Ionicons] is used for corporate logos (Twitter, GitHub, etc.)
2. [Heroicons] is used for everything else. In general the outline variants are preferred.

### Redirects

Redirects for vector.dev are defined in three difference places (depending on the use):

1. Domain-level redirects, e.g. the chat.vector.dev redirect to our Discord server, are defined in [`netlify.toml`](../netlify.toml) in the repo root.
2. Splat-style redirects (which can't be defined as Hugo aliases) are defined in [`./static/_redirects`](./static/_redirects).
3. Redirects for specific pages are defined in the [`aliases`][aliases] field in the relevant page's front matter.

### Link checking

vector.dev uses the [htmltest] link checker to sniff out broken links. Whenever the site is built in CI, be it the preview build or the production build, all internal links on the site are checked. If *any* link is broken, the build fails. If you push changes to the docs/site and your build yields a big red X but running the site locally works fine, scan the CI output for broken links. There's a good chance that that's your culprit.

You can run the full CI builds locally, with link checking included:

```shell
# Production
make local-production-build

# Preview
make local-preview-build
```

The standard link checking configuration is in [`.htmltest.yml`](./.htmltest.yml). As you can see from this config, external links are *not* checked (`CheckExternal: false`). That's because external link checking makes builds highly brittle, as they become dependent upon the availability of external websites, i.e. if CloudFlare has an outage or Wikipedia goes down, the vector.dev build fails. The trade-off here, of course, is that broken external links can go undetected. The half-solution is to periodically run ad hoc external link checks:

```shell
make local-production-build
make run-external-link-checker
```

That second make command runs htmltest using the [`.htmltest.external.yml`](./htmltest.external.yml) configuration, which sets `CheckExternal` to `true`. We should strive to run this periodically in local environments to make sure we don't have too much drift over time.

## Tasks

Below is a list of common tasks that maintainers will need to carry out from time to time.

### Run the site locally

```shell
make serve
```

This builds all the necessary [prereqs](#prerequisites) for the site and starts up a local web server. Navigate to http://localhost:1313 to view the site.

When you make changes to the Markdown sources, Sass/CSS, or JavaScript, the site re-builds and Hugo automatically reloads the page that you're on. If you make changes to the [structured data](#structured-data) sources, however, you need to stop the server and run `make serve` again.

### Add a new version of Vector

1. Add the new version to the `versions` list in [`cue/reference/versions.cue`](./cue/reference/versions.cue). Make sure to preserve reverse ordering.
1. Generate a new CUE file for the release by running `make release-prepare` in the root directory of the Vector repo. This generates a CUE file at `cue/releases/{VERSION}.cue`.
1. Add a new Markdown file to [`content/en/releases`](./content/en/releases), where the filename is `{version}.md` (e.g. `0.12.0.md`) and the file has metadata that looks like this:

    ```markdown
    ---
    title: Vector v0.13.0 release notes
    weight: 19
    ---
    ```

    The `title` should reflect the version, while the `weight` should be the weight of the next most recent version plus 1. The file for version 0.8.1, for example, has a weight of 8, which means the weight for version 0.8.2 (the next higher version) is 9. This metadata is necessary because Hugo can't sort semantic versions, so we need to make the ordering explicit. If Hugo ever does allow for semver sorting, we should remove the `weight`s.

## Lighthouse scores

[Lighthouse] scores for the website are produced automatically by [Netlify's Lighthouse plugin][plugin]. Those reports are available at `${ROOT}/reports/lighthouse`, where `ROOT` is the root URL for a version of the site. Thus, reports for the production version of the site would be available at https://vector.dev/reports/lighthouse. Reports are also generated for deploy previews and branch deploys.

## Known issues

* Tailwind's [typography] plugin is used to render text throughout the site. It's a decent library in general but is also rather buggy, with some rendering glitches in things like lists and tables that we've tried to compensate for in the `extend.typography` block in the [Tailwind config](./tailwind.config.js), but it will take some time to iron all of these issues out.

## CUE pro tips

[CUE] can be tricky, tripping up even the most seasoned veterans of the language. Below are some tips that might help you get over the hump with whatever CUE logic you're trying to add to the Vector docs.

### One step at a time

We generally advise writing CUE in an incremental way. If you add a lot of new CUE logic and _then_ validate what you've added, the likelihood of encountering inscrutable errors and having little insight into where specifically you went wrong is quite high. Instead, add and then validate little bits at a time. Tools like [`watchexec`][watchexec] can help with this. Here's an example command (run here in the `website` directory):

```shell
watchexec "make cue-build"
```

This runs the CUE build every time you save a change to your CUE sources. The feedback loop is typically 2-5 seconds.

### Watch your indentation

Good:

```cue
description: """
    Here is a long string...
    """
```

Bad:

```cue
description: """
        Here is a long string...
    """
```

Also bad:

```cue
description: """
    Here is a long string...
        """
```

[typesense]: https://typesense.org
[aliases]: https://gohugo.io/content-management/urls
[alpine]: https://alpinejs.dev
[components]: https://vector.dev/components
[cue]: https://cuelang.org
[deploy previews]: https://docs.netlify.com/site-deploys/deploy-previews
[directory structure]: https://gohugo.io/getting-started/directory-structure
[esbuild]: https://github.com/evanw/esbuild
[guides]: https://vector.dev/guides
[heroicons]: https://heroicons.com
[htmltest]: https://github.com/wjdp/htmltest
[hugo]: https://gohugo.io
[hugo pipes]: https://gohugo.io/hugo-pipes
[ionicons]: https://ionic.io/ionicons
[lighthouse]: https://web.dev/performance-scoring
[netlify]: https://netlify.com
[netlify_project]: https://app.netlify.com/sites/vector-project/overview
[node.js]: https://nodejs.org
[plugin]: https://www.npmjs.com/package/@netlify/plugin-lighthouse
[postcss]: https://github.com/postcss/postcss
[purgecss]: https://purgecss.com
[react.js]: https://reactjs.org
[reference documentation]: https://vector.dev/docs/reference
[sass]: https://sass-lang.com
[spruce]: https://spruce.ryangjchandler.co.uk
[tailwind]: https://tailwindcss.com
[tocbot]: https://tscanlin.github.io/tocbot
[typescript]: https://www.typescriptlang.org
[typography]: https://github.com/tailwindlabs/tailwindcss-typography
[vector]: https://vector.dev
[vrl]: https://vrl.dev
[watchexec]: https://github.com/watchexec/watchexec
[yarn]: https://yarnpkg.com
