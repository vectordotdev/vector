Reduce transforms can now properly aggregate nested fields.
This is a breaking change because previously, merging object elements used the "discard" strategy.
The new behavior is to use the default strategy based on the element type.

### Example

#### Event 1

```json
{
  "id": 777,
  "an_array": [
    {
      "inner": 1
    }
  ],
  "message": {
    "a": {
      "b": [1, 2],
      "num": 1
    }
  }
}
```

#### Event 2

```json
{
  "id": 777,
  "an_array": [
    {
      "inner": 2
    }
  ],
  "message": {
    "a": {
      "b": [3, 4],
      "num": 2
    }
  }
}
```

#### Reduced Event

Old behavior:
```json
{
  "id": 777,
  "an_array": [
    {
      "inner": 2
    }
  ],
  "message": {
    "a": {
      "b": [1, 2],
      "num": 1
    }
  }
}
```

New behavior:
```json
{
  "id": 777,
  "an_array": [
    {
      "inner": 1
    }
  ],
  "message": {
    "a": {
      "b": [
        [1, 2],
        [3,4]
      ],
      "num": 3
    }
  }
}
```
