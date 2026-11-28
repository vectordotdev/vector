The `loki` sink has a new `expanded_key_case` option. Set it to `preserve` to keep label and structured metadata keys expanded from `*` (or a prefix ending with `*`) in their original case, so keys such as `userId` and `userid` no longer collide. The default, `lowercase`, keeps the current behavior.

authors: patsroka
