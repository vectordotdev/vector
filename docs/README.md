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

The [`website`][website_branch] branch is used to build the "production" site at https://vector.dev. If you want to update the production site, make sure to target your changes to that branch.

The `master` branch, on the other hand, often contains unreleased, "nightly" changes to Vector that we don't yet want reflected in the website. The nightly version of the site is available at https://master.vector.dev. This version of the site may be useful for Vector users taking advantage of not-yet-released features. The `master` and `website` branches should be synced

### Static site generator

vector.dev is built using the [Hugo] static site generator. The site configuration is in [`config.toml`](./config.toml). The standard Hugo [directory structure] is obeyed.

### Structured data

The Vector documentation relies heavily on structured data supplied using the [CUE] configuration and data validation language. Uses of CUE data include the docs for Vector's many [components] and the docs for [Vector Remap Language][vrl].

All of the CUE sources for the site are in the [`cue`](./cue) directory. Whenever you build the Vector site, the CUE sources are compiled into a single JSON file that's stored at `data/docs.json`. That information is then used in conjunction with Hugo's templating system to build HTML.

There's a variety of helper commands available for working with CUE. Run `make cue-help` for CLI docs.

### JavaScript

For the most part, vector.dev uses the [Alpine] framework for interactive functionality. If you see directives like `x-show`, `x-data`, `@click`, and `:class` in HTML templates, those are Alpine directives. Alpine was chosen over jQuery and other frameworks for the sake of maintainability. Alpine directives live inside your HTML rather than in separate JavaScript files, which enables you to see how a component behaves without referring to an external `.js` file.

The [Spruce] library is used for all JavaScript state management. It stores things like light/dark mode preferences in `localStorage` and makes those values available in Alpine-wired components. See the [`app.js`](./assets/js/app.js) for managed state values.

The [Tocbot] library is used to auto-generate documentation table of contents on each page. The TOC is generated at page load time.

You'll also find two [React.js] components on the site: the spinning globe on the main page and the interactive search bar. The [TypeScript] for those components is in [`home.tsx`](./assets/js/home.tsx) and [`search.tsx`](./assets/js/search.tsx), respectively. React.js compilation is configured using the [`babel.config.js`](./babel.config.js) file and TypeScript compilation is configured using the [`tsconfig.json`](./tsconfig.json) file.

All JavaScript for the site is built using [Hugo Pipes] rather than tools like Webpack, Gulp, or Parcel.

### CSS

Most of the site's CSS is provided by [Tailwind], which is a framework based on CSS utility classes. The Tailwind configuration is in [`tailwind.config.js`](./tailwind.config.js); it mostly consists of default values but there are some custom colors, sizes, and other attributes provided there.

CSS post-processing is performed by [PostCSS], which is configured via the [`postcss.config.js`](./postcss.config.js) file.

### Search

### Redirects

Redirects for vector.dev are defined in three difference places (depending on the use):

1. Domain-level redirects, e.g. chat.vector.dev to our Discord server, are defined in [`netlify.toml`](../netlify.toml) in the repo root.
2. Splat-style redirects (which can't be defined as Hugo aliases) are defined in [`./static/_redirects`](./static/_redirects).
3. Redirects for specific pages are defined in the `aliases` field in that page's front matter.

### Link checking

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

## Known issues

* Tailwind's [typography] plugin is used to render text throughout the site. It's a decent library in general but is also rather buggy, with some rendering glitches in things like lists and tables that we've tried to compensate for in the `extend.typography` block in the [Tailwind config](./tailwind.config.js), but it will take some time to iron all of these issues out.

[alpine]: https://alpinejs.dev
[components]: https://vector.dev/components
[cue]: https://cue-lang.org
[deploy previews]: https://docs.netlify.com/site-deploys/deploy-previews
[directory structure]: https://gohugo.io/getting-started/directory-structure
[esbuild]: https://github.com/evanw/esbuild
[guides]: https://vector.dev/guides
[htmltest]: https://github.com/wjdp/htmltest
[hugo]: https://gohugo.io
[hugo pipes]: https://gohugo.io/hugo-pipes
[netlify]: https://netlify.com
[netlify_project]: https://app.netlify.com/sites/vector-project/overview
[node.js]: https://nodejs.org
[postcss]: https://github.com/postcss/postcss
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
[website_branch]: https://github.com/timberio/vector/tree/website
[yarn]: https://yarnpkg.com
