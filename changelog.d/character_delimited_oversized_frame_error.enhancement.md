The `character_delimited` and `newline_delimited` decoding framers now report frames dropped for
exceeding `max_length` (`oversized_action: drop`) as an error: the message is logged at `error`
level and `component_errors_total` is incremented with `error_code="decoder_frame_too_long"`.
Previously these drops were only logged as a warning. Dropping behavior is otherwise unchanged.

authors: taylorchandleryoung
