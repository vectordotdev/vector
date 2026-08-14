Allows hyphens in the `<backend name>` portion of the `SECRET[<backend name>.<secret name>]` collector regex. Before, a backend name containing a hyphen (e.g. `my-backend`) would fail to match, leaving the literal `SECRET[...]` string in the resolved config instead of the secret value.

authors: maklean
