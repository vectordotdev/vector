protobuf decoder will no longer set fields that are not set in the incoming byte stream. This to
ensure that the encoder will return the exact same bytes for the same given event.
