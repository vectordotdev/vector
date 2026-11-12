// Protocol Buffers field numbers for series and sketch payload messages in the V3 format.
//
// These field numbers come from the Protocol Buffers definitions in `lib/protos/datadog/proto/agent-payload/intake_v3.proto`.
pub(crate) const DICT_NAME_STR_FIELD_NUMBER: u32 = 1;
pub(crate) const DICT_TAGS_STR_FIELD_NUMBER: u32 = 2;
pub(crate) const DICT_TAGSETS_FIELD_NUMBER: u32 = 3;
pub(crate) const DICT_RESOURCE_STR_FIELD_NUMBER: u32 = 4;
pub(crate) const DICT_RESOURCE_LEN_FIELD_NUMBER: u32 = 5;
pub(crate) const DICT_RESOURCE_TYPE_FIELD_NUMBER: u32 = 6;
pub(crate) const DICT_RESOURCE_NAME_FIELD_NUMBER: u32 = 7;
pub(crate) const DICT_SOURCE_TYPE_NAME_FIELD_NUMBER: u32 = 8;
pub(crate) const DICT_ORIGIN_INFO_FIELD_NUMBER: u32 = 9;
pub(crate) const TYPES_FIELD_NUMBER: u32 = 10;
pub(crate) const NAMES_FIELD_NUMBER: u32 = 11;
pub(crate) const TAGS_FIELD_NUMBER: u32 = 12;
pub(crate) const RESOURCES_FIELD_NUMBER: u32 = 13;
pub(crate) const INTERVALS_FIELD_NUMBER: u32 = 14;
pub(crate) const NUM_POINTS_FIELD_NUMBER: u32 = 15;
pub(crate) const TIMESTAMPS_FIELD_NUMBER: u32 = 16;
pub(crate) const VALS_SINT64_FIELD_NUMBER: u32 = 17;
pub(crate) const VALS_FLOAT32_FIELD_NUMBER: u32 = 18;
pub(crate) const VALS_FLOAT64_FIELD_NUMBER: u32 = 19;
pub(crate) const SKETCH_NUM_BINS_FIELD_NUMBER: u32 = 20;
pub(crate) const SKETCH_BIN_KEYS_FIELD_NUMBER: u32 = 21;
pub(crate) const SKETCH_BIN_CNTS_FIELD_NUMBER: u32 = 22;
pub(crate) const SOURCE_TYPE_NAME_FIELD_NUMBER: u32 = 23;
pub(crate) const ORIGIN_INFO_FIELD_NUMBER: u32 = 24;
pub(crate) const DICT_UNIT_STR_FIELD_NUMBER: u32 = 25;
pub(crate) const UNIT_REFS_FIELD_NUMBER: u32 = 26;

/// Display names for the V3 columns, indexed by their Protocol Buffers field number (from
/// `lib/protos/datadog/proto/agent-payload/intake_v3.proto`, also mirrored in
/// `datadog_agent_metrics_v3::writer` as the `*_FIELD_NUMBER` constants). Index 0 is unused since
/// field numbers start at 1.
pub const COLUMN_NAMES: [&str; 27] = [
    "reserved",
    "DictNameStr",
    "DictTagsStr",
    "DictTagsets",
    "DictResourceStr",
    "DictResourcesLen",
    "DictResourceType",
    "DictResourceName",
    "DictSourceTypeName",
    "DictOriginInfo",
    "Type",
    "Name",
    "Tags",
    "Resources",
    "Interval",
    "NumPoints",
    "Timestamp",
    "ValueSint64",
    "ValueFloat32",
    "ValueFloat64",
    "SketchNBins",
    "SketchBinKeys",
    "SketchBinCounts",
    "SourceTypeName",
    "OriginInfo",
    "DictUnitStr",
    "UnitRef",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_numbers_match_column_names() {
        let pairs: &[(u32, &str)] = &[
            (DICT_NAME_STR_FIELD_NUMBER, "DictNameStr"),
            (DICT_TAGS_STR_FIELD_NUMBER, "DictTagsStr"),
            (DICT_TAGSETS_FIELD_NUMBER, "DictTagsets"),
            (DICT_RESOURCE_STR_FIELD_NUMBER, "DictResourceStr"),
            (DICT_RESOURCE_LEN_FIELD_NUMBER, "DictResourcesLen"),
            (DICT_RESOURCE_TYPE_FIELD_NUMBER, "DictResourceType"),
            (DICT_RESOURCE_NAME_FIELD_NUMBER, "DictResourceName"),
            (DICT_SOURCE_TYPE_NAME_FIELD_NUMBER, "DictSourceTypeName"),
            (DICT_ORIGIN_INFO_FIELD_NUMBER, "DictOriginInfo"),
            (TYPES_FIELD_NUMBER, "Type"),
            (NAMES_FIELD_NUMBER, "Name"),
            (TAGS_FIELD_NUMBER, "Tags"),
            (RESOURCES_FIELD_NUMBER, "Resources"),
            (INTERVALS_FIELD_NUMBER, "Interval"),
            (NUM_POINTS_FIELD_NUMBER, "NumPoints"),
            (TIMESTAMPS_FIELD_NUMBER, "Timestamp"),
            (VALS_SINT64_FIELD_NUMBER, "ValueSint64"),
            (VALS_FLOAT32_FIELD_NUMBER, "ValueFloat32"),
            (VALS_FLOAT64_FIELD_NUMBER, "ValueFloat64"),
            (SKETCH_NUM_BINS_FIELD_NUMBER, "SketchNBins"),
            (SKETCH_BIN_KEYS_FIELD_NUMBER, "SketchBinKeys"),
            (SKETCH_BIN_CNTS_FIELD_NUMBER, "SketchBinCounts"),
            (SOURCE_TYPE_NAME_FIELD_NUMBER, "SourceTypeName"),
            (ORIGIN_INFO_FIELD_NUMBER, "OriginInfo"),
            (DICT_UNIT_STR_FIELD_NUMBER, "DictUnitStr"),
            (UNIT_REFS_FIELD_NUMBER, "UnitRef"),
        ];

        for (field_number, expected_name) in pairs {
            assert_eq!(
                COLUMN_NAMES[*field_number as usize], *expected_name,
                "field number {field_number} should index COLUMN_NAMES to \"{expected_name}\""
            );
        }
    }
}
