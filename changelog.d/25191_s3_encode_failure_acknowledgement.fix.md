The `aws_s3` sink now rejects a batch when encoding it fails, so sources with end-to-end acknowledgements enabled are told the data was not delivered. Previously an encoding failure was only logged and the batch was acknowledged as delivered, which hid the data loss from the upstream source.

authors: pedramsafaei
