Events rejected by a full buffer configured with `when_full: drop_newest`, or by
a disk buffer that cannot encode or fit the record, now receive an error
acknowledgement instead of a successful delivery acknowledgement. Previously
accepted events remain buffered and subsequent valid records can still be written.

authors: fernandol-nvidia
