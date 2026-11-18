"""Small independent KPL/proto2 decoder for local verification, not timed Rust code.

Schema: awslabs/amazon-kinesis-producer/aggregation-format.md. The receiver
uses no encoding helpers or size calculations from the Rust implementation.
"""

import hashlib


def _varint(data, offset):
    value = 0
    for shift in range(0, 70, 7):
        if offset >= len(data):
            raise ValueError("truncated protobuf varint")
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if byte < 128:
            if value >= 1 << 64:
                raise ValueError("protobuf varint exceeds uint64")
            return value, offset
    raise ValueError("unterminated protobuf varint")


def _fields(data):
    offset = 0
    while offset < len(data):
        tag, offset = _varint(data, offset)
        field, wire = tag >> 3, tag & 7
        if not field:
            raise ValueError("invalid protobuf field zero")
        if wire == 0:
            value, offset = _varint(data, offset)
        elif wire in (1, 2, 5):
            if wire == 2:
                size, offset = _varint(data, offset)
            else:
                size = 8 if wire == 1 else 4
            end = offset + size
            if end > len(data):
                raise ValueError("truncated protobuf field")
            value, offset = data[offset:end], end
        else:
            raise ValueError("unsupported protobuf wire type")
        yield field, wire, value


def decode_kpl(data):
    if len(data) < 20 or data[:4] != b"\xf3\x89\x9a\xc2":
        raise ValueError("missing KPL envelope")
    message = data[4:-16]
    if hashlib.md5(message).digest() != data[-16:]:
        raise ValueError("KPL checksum mismatch")
    keys, hashes, records = [], [], []
    for field, wire, value in _fields(message):
        if field in (1, 2, 3):
            if wire != 2:
                raise ValueError("invalid KPL aggregate field type")
            if field == 1:
                keys.append(value.decode("utf-8"))
            elif field == 2:
                hashes.append(value.decode("ascii"))
            else:
                records.append(value)
    result = []
    for record in records:
        key_index = hash_index = payload = None
        for field, wire, value in _fields(record):
            if field == 1 and wire == 0:
                key_index = value
            elif field == 2 and wire == 0:
                hash_index = value
            elif field == 3 and wire == 2:
                payload = value
        if key_index is None or key_index >= len(keys) or payload is None:
            raise ValueError("missing payload or invalid KPL partition-key index")
        if hash_index is not None and hash_index >= len(hashes):
            raise ValueError("invalid KPL explicit-hash-key index")
        result.append((keys[key_index], None if hash_index is None else hashes[hash_index], payload))
    if not result:
        raise ValueError("empty KPL aggregate")
    return result
