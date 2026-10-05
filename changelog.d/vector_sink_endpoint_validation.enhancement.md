The `vector` sink now rejects empty, host-less, or non-`http(s)` `address` and `routing.endpoints` values at configuration load with a clear error, including with `vector validate --no-environment`.

authors: thomasqueirozb
