# Generate

The `make generate` command auto-generates files across the Vector repository
(mostly documentation). This folder contains all of the inner-workings of that
command.

## Metadata

To start, you should be familiar with the Vector [`/.meta`](/.meta) directory.
This directory contains various metadata about the Vector project as a whole,
mostly configuration details but also link definitions and more. Its content is
loaded via the [`./util/metadata.rb`](./util/metadata.rb) file and represented
as an object.

## Templates

If a file in the Vector repo needs to be dynamically generated you can place
it in the [templates](templates) directory. The structure of this directory
should match the root Vector structure exactly, and only include files that
need to be generated. For example, the [`/README.md`](/README.md) is generated
by the [`/scripts/generate/templates/README.md.erb`](/scripts/generate/templates/README.md.erb)
template.

## Context

Context refers to the execution context used when rendering templates. This
is represented by the [`context.rb`](context.rb) file. All methods in this
file are available within templates. This is a single global context used in
all template files.

## Partials

The [`context.rb`](context.rb) file incudes a `#render_partial` method that can
be used to render partial templates that are commonly reused. These partials are
placed in [`templates/_partials`](templates/partials).

Partials should only be used when absolutely necessary.
