Fixed disk buffer corruption caused by deeply nested events exceeding prost's protobuf recursion limit during decode. Events with nesting depth greater than 32 are now rejected at encode time across disk buffers, the native codec, and the `vector` sink's gRPC path, preventing unrecoverable buffer corruption.

authors: connoryy
