Fixed a bug in the URI confinement check for `{{ field }}` templates where the dynamic reference sat inside (or directly extended) the authority component, e.g. `https://tenant.{{ env }}.example.com/` or `https://api.internal{{ path }}`. Previously, this built a confinement baseline from a truncated, non-authority-closed host, causing every render to fail the authority match and silently drop all events. Such templates are now rejected at build time.

authors: thomasqueirozb
