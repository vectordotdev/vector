# Debian example configurations moved out of /etc/vector {#deb-examples-relocation}

## Summary

The Debian package now installs its example configurations to `/usr/share/doc/vector/examples/` instead of `/etc/vector/examples/`. Examples are documentation rather than administrator-managed configuration, so `/etc` was the wrong location for them.

## Migration

No action is required for most users. The examples are not read by Vector; they are reference material only, and the new copies are installed automatically on upgrade.

Because the old paths were dpkg conffiles, the upgrade removes unmodified copies from `/etc/vector/examples/` and leaves any file you edited in place, so local changes are never discarded silently. If you edited an example and want to keep it, move it somewhere outside `/etc/vector/examples/`; if you referenced one from your own configuration with a path under `/etc/vector/examples/`, update that path to `/usr/share/doc/vector/examples/`.

authors: koenserry
