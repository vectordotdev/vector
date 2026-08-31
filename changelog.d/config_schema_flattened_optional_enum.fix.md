Fixed generated configuration schemas for flattened optional internally-tagged enums. Configs that omit the flattened block now validate: the schema encodes `None` as a missing tag field rather than JSON `null`, matching `serde`.

authors: bruceg
