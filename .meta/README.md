# Meta Directory

The `.meta` directory represents metadata for the Vector project as a whole.
It is primarily used to auto-generate files in the
[`vector-website`](https://github.com/timberio/vector-website) repo. You do not
need to concern yourself with generating files in that repository, instead
you can validate your changes with a simple command:

    make check-meta

If validation passes, that's it! Changes will be merged and updated manually
in the `vector-website` repo by the Vector team.
