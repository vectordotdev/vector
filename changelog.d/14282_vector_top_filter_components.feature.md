`vector top` now supports filtering out components by their component ID using glob patterns with a new `--components` option.
This is very similar to `vector tap` `--outputs-of` and `--inputs-of` options. This can be useful
in cases where you have a lot of components and they don't fit in the terminal (as scrolling is not supported yet in `vector top`).
By default, all components are shown with a glob pattern of `*`.

The glob pattern semantics can be found in the [`glob` crate documentation](https://docs.rs/glob/latest/glob/).

Example usage: `vector top --components "demo*"` will only show the components that match the glob pattern `demo*`.

authors: jorgehermo9
