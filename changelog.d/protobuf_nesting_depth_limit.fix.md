Fixed unrecoverable disk buffer corruption and vector-to-vector retry loops caused by event data or metadata that protobuf could encode but prost could not decode. Vector now rejects only protobuf-unsafe nested payloads before disk buffer, native codec, or `vector` sink gRPC encoding, while preserving nested shapes that prost can safely decode.

authors: connoryy
