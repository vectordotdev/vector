Fixed an incorrect file-ID calculation in the disk buffer (`disk_v2`) ledger. `get_offset_reader_file_id` performed the offset addition in `u16`, which could wrap before the modulo was applied (when `MAX_FILE_ID` is `u16::MAX`), causing distinct offsets to resolve to the same data file ID. The calculation now uses wider arithmetic so it wraps correctly.

authors: xfocus3
