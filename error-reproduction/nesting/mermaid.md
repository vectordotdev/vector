---                                                                                                                                                                    
title: "Disk Buffer: InvalidProtobufPayload Root Cause"                                                                                                                
---                                                                                                                                                                    
flowchart TD                                                                                                                                                           
%% ═══════════════════════════════════════════                                                                                                                     
%% CONVENTIONS                                                                                                                                                     
%% Rectangle  [""]  = object instance                                                                                                                              
%% Diamond    {""}  = decision / conditional
%% Stadium    ([""]) = terminal state
%% Cylinder   [("")] = persistent storage
%% Solid arrow  -->  = method call
%% Dotted arrow -.-> = data flow / annotation
%% ═══════════════════════════════════════════

subgraph WRITE["WRITE PATH"]
    direction TB

    W_RW["RecordWriter
    writer.rs:372"]

    W_EA["EventArray
    ser.rs:81"]

    W_PROTO["proto::EventArray
    prost generated"]

    W_BUF["encode_buf — bytes P
    writer.rs:412"]

    W_REC["Record
    record.rs:51-73
    ─────────────
    .checksum: CRC32 of id+meta+P
    .id: u64
    .metadata: u32
    .payload: bytes P"]

    W_DISK[("Data File
    on disk")]

    W_RW -->|".archive_record(id, record)
    writer.rs:450"| W_EA

    W_EA -->|".encode(&mut buf)
    ser.rs:94-100"| W_PROTO

    W_PROTO -->|"prost Message::encode()
    ✅ NO RECURSION LIMIT"| W_BUF

    W_BUF -->|"Record::with_checksum()
    record.rs:121-129"| W_REC

    W_REC -->|"rkyv Serialize + write
    writer.rs:499-510"| W_DISK
end

W_DISK -.->|"same bytes on disk"| R_DISK

subgraph READ["READ PATH"]
    direction TB

    R_DISK[("Data File
    on disk")]

    R_RR["RecordReader
    reader.rs:198
    ─────────────
    .aligned_buf: AlignedVec
    .checksummer: Hasher"]

    R_DISK -->|".fill_buf() + read N bytes
    reader.rs:322-334"| R_RR

    %% ── Gate 1: rkyv ──

    R_RR -->|"check_archived_root‹Record›(buf)
    record.rs:193 → ser.rs:88"| D_RKYV{"rkyv archive
    structurally valid?"}

    D_RKYV -->|no| E_DESER(["FailedDeserialization
    record.rs:27
    → skip entire file"])

    D_RKYV -->|yes| R_AR["ArchivedRecord
    record.rs:132"]

    %% ── Gate 2: checksum ──

    R_AR -->|".verify_checksum(hasher)
    record.rs:144-154"| D_CRC{"CRC32(id + meta + payload)
    == stored checksum?"}

    D_CRC -->|no| E_CORRUPT(["Corrupted
    record.rs:25
    → skip entire file"])

    D_CRC -->|"yes"| R_TOKEN["ReadToken
    reader.rs:30-33
    ─────────────
    Under A1 + A2:
    payload bytes B == bytes P"]

    %% ── Gate 3: metadata ──

    R_TOKEN -->|".read_record(token)
    reader.rs:364-378"| R_AR2["ArchivedRecord
    record.rs:132
    ─────────────
    .payload() → bytes B
    record.rs:139-141"]

    R_AR2 -->|"decode_record_payload(record)
    reader.rs:1135-1155"| D_META{"T::Metadata::from_u32()
    and T::can_decode()?
    reader.rs:1140-1144"}

    D_META -->|no| E_INCOMPAT(["Incompatible
    reader.rs:1145"])

    D_META -->|yes| R_DEC["Encodable::decode impl
    for EventArray
    ser.rs:103-118"]

    %% ── Attempt 1 ──

    R_DEC -->|"proto::EventArray::decode(B.clone())
    ser.rs:108"| R_PA["proto::EventArray
    prost Message::decode
    ❌ RECURSION_LIMIT = 100"]

    R_PA --> D_EA{"EventArray
    decode succeeded?"}

    D_EA -->|yes| S_OK1(["Ok — return EventArray"])

    %% ── Attempt 2 ──

    D_EA -->|"no — Err discarded
    .or_else ser.rs:110"| R_PW["proto::EventWrapper
    prost Message::decode
    ❌ RECURSION_LIMIT = 100"]

    R_PW -->|"proto::EventWrapper::decode(B)
    ser.rs:111"| D_EW{"EventWrapper
    decode succeeded?"}

    D_EW -->|yes| S_OK2(["Ok — return EventArray"])

    D_EW -->|"no — Err discarded
    .map_err ser.rs:113"| E_IPP(["DecodeError::InvalidProtobufPayload
    ser.rs:113
    ──────────
    SOLE PRODUCTION SITE"])
end

%% ═══ STYLING ═══
style WRITE fill:#f0f4ff,stroke:#4263eb,stroke-width:2px,color:#000
style READ fill:#fff5f5,stroke:#e03131,stroke-width:2px,color:#000

style W_PROTO fill:#d0ebff,stroke:#1971c2
style R_PA fill:#ffe3e3,stroke:#c92a2a
style R_PW fill:#ffe3e3,stroke:#c92a2a

style S_OK1 fill:#b2f2bb,stroke:#2f9e44,color:#000
style S_OK2 fill:#b2f2bb,stroke:#2f9e44,color:#000

style E_DESER fill:#ffe8cc,stroke:#e8590c,color:#000
style E_CORRUPT fill:#ffe8cc,stroke:#e8590c,color:#000
style E_INCOMPAT fill:#ffe8cc,stroke:#e8590c,color:#000
style E_IPP fill:#ff6b6b,stroke:#c92a2a,color:#fff,stroke-width:3px