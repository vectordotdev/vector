Fixed the `reduce` transform applying a merge strategy twice when the field name has to be
quoted in a path, such as `"a-b"` or `parent."a-b"`. For example, `sum` over the values `2` and
`3` returned `10` instead of `5`. Fields nested under such a strategy path are no longer merged a
second time either. The check for fields covered by a merge strategy also no longer allocates
for every field of every event.

authors: ckelner
