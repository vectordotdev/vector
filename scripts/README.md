# `scripts` folder

The `scripts` folder contains various utility scripts that help to
maintain the Vector project. All of these are exposed through the `Makefile`,
and it should be rare that you have to call these directly.

## Setup

### Using Docker

All make targets run through Docker by default.

### Without Docker

If you do not wish to use Docker then you'll need to intsall Ruby 2.7 locally.
Then, when running `make` commands you can disable Docker with with
`USE_CONTAINER` environment variable:

```bash
USE_CONTAINER=none make generate
```
