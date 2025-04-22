Add option to run `ends_when` condition on the merged event in reduce transform which allows for things such as dynamic message count, etc

An optional tag is added to the reduce config to be able to use the merged event.
The default is to operate on an incoming event.

New config:

```yaml
transforms:
  my_transform_id:
    type: reduce
    inputs:
      - my-source-or-transform-id
    ends_when:
      apply_to: "merged_event"
      type: "vrl"
      source: "(is_array(.message) && (to_int!(._extra.size)) == length!(.message)) || (is_string(.message) && to_int!(._extra.size) == 1)"
```

For incoming event, you can use the current syntax or

```yaml
transforms:
  my_transform_id:
    type: reduce
    inputs:
      - my-source-or-transform-id
    ends_when:
      apply_to: "incoming_event"
      type: "vrl"
      source: ".status == 500"
```

authors: Goggin
