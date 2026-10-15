Transforms and task transforms that receive no events now release removed or replaced downstream
components on config reload. Previously an idle upstream (for example a `filter` that rarely passes
events) kept the old downstream component alive until its next event, leaking memory on every
reload, and could block a reload that replaced a sink with a resource conflict.

authors: fingon
