The `kafka` sink now sends a message with no key when the `key_field` value is `null`, as it already did when the field is missing, instead of the string `"<null>"`.

authors: anpawo
