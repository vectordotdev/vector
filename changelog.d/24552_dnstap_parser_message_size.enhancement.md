Adds new field to parsed dnstap data: `rawDataSize`. It is always present, both in cases when only `rawData` is emitted and when the whole packet is correctly parsed. It represents the size of the incoming dnstap frame.

authors: esensar Quad9DNS
