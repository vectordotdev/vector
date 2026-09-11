//! Decision helpers for completing closed Kinesis shards after a reshard.
//!
//! After a split/merge, parent shards are CLOSED (`EndingSequenceNumber` set). Polling
//! them at the tip (especially with `LATEST`) makes CloudWatch
//! `GetRecords.IteratorAgeMilliseconds` climb 1:1 with wall clock until retention
//! expires the shard. These helpers decide iterator type, lease eligibility
//! (KCL 3.0 parent-before-child), when the consumer is finished, and whether a
//! failed `GetShardIterator` should delete the DynamoDB row or write `SHARD_END`.

use std::collections::{HashMap, HashSet};

/// Consecutive empty GetRecords polls on a closed shard where `MillisBehindLatest`
/// only increased. Two is enough to distinguish a frozen tip from a single blip.
pub const GROWING_AGE_POLLS_TO_FINISH: u32 = 2;

/// KCL `ExtendedSequenceNumber.SHARD_END`. Written when a shard is fully consumed
/// so children can start, then deleted after children have their own leases.
pub const SHARD_END: &str = "SHARD_END";

/// A shard is closed (parent after split/merge) when it has an ending sequence
/// number that is not the literal string `"null"` (some ListShards payloads).
pub fn is_shard_closed(ending_sequence_number: Option<&str>) -> bool {
    ending_sequence_number.map(|e| e != "null").unwrap_or(false)
}

pub fn is_shard_end(sequence: &str) -> bool {
    sequence == SHARD_END
}

/// Parent + adjacent-parent IDs from ListShards (`ParentShardId` / `AdjacentParentShardId`).
pub fn parent_ids(parent: Option<&str>, adjacent: Option<&str>) -> Vec<String> {
    [parent, adjacent]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty() && *s != "null")
        .map(ToOwned::to_owned)
        .collect()
}

pub fn is_child_shard(parent_ids: &[String]) -> bool {
    !parent_ids.is_empty()
}

/// KCL `BlockOnParentShardTask`: a child may start when every parent has no
/// checkpoint (lease never existed / already trimmed) or is checkpointed `SHARD_END`.
pub fn parents_completed(parent_ids: &[String], sequences: &HashMap<String, String>) -> bool {
    parent_ids.iter().all(|parent| match sequences.get(parent) {
        None => true,
        Some(seq) => is_shard_end(seq),
    })
}

/// Whether this shard may be leased this cycle (KCL 3.0 eligibility).
///
/// Never claim `SHARD_END`. Never claim a child until its parents are completed.
/// Closed shards without a checkpoint are skipped (no work, no leftover row).
/// Closed shards **with** a leftover checkpoint are claimed so remaining records
/// can be drained instead of polled at `LATEST`.
pub fn should_claim_shard(
    shard_closed: bool,
    has_checkpoint: bool,
    sequence: Option<&str>,
    parent_ids: &[String],
    sequences: &HashMap<String, String>,
) -> bool {
    if sequence.map(is_shard_end).unwrap_or(false) {
        return false;
    }
    if !parents_completed(parent_ids, sequences) {
        return false;
    }
    if shard_closed && !has_checkpoint {
        return false;
    }
    true
}

/// KCL `LeaseCleanupManager`: drop a `SHARD_END` parent once every child listed
/// by ListShards has its own checkpoint. An empty child list means the parent is
/// still listed but children have not appeared yet — keep the row.
pub fn should_delete_shard_end_lease(
    sequence: &str,
    child_ids: &[String],
    sequences: &HashMap<String, String>,
) -> bool {
    if !is_shard_end(sequence) || child_ids.is_empty() {
        return false;
    }
    child_ids.iter().all(|child| sequences.contains_key(child))
}

/// Checkpoints for shard IDs that ListShards no longer returns (retention expired).
pub fn leftover_checkpoint_shard_ids(
    checkpoint_shard_ids: impl IntoIterator<Item = String>,
    listed_shard_ids: &HashSet<String>,
) -> Vec<String> {
    checkpoint_shard_ids
        .into_iter()
        .filter(|id| !listed_shard_ids.contains(id))
        .collect()
}

/// Map each parent shard ID to the child shard IDs that name it as a parent.
pub fn children_by_parent(shards: &[(String, Vec<String>)]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (shard_id, parents) in shards {
        for parent in parents {
            map.entry(parent.clone())
                .or_default()
                .push(shard_id.clone());
        }
    }
    map
}

/// Which `GetShardIterator` type to request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShardIteratorChoice {
    AfterSequenceNumber,
    TrimHorizon,
    Latest,
}

/// Choose a shard iterator type.
///
/// Closed shards must never use `LATEST`: that parks the iterator at the last
/// record and iterator age grows until retention. Empty sequence on a closed
/// shard uses `TRIM_HORIZON` so remaining records are drained.
///
/// Child shards also use `TRIM_HORIZON` when they have no sequence, matching
/// KCL 3.0 (initial position on a child is TRIM_HORIZON even if the configured
/// position is LATEST, so records are not skipped after a reshard).
pub fn shard_iterator_choice(
    sequence: &str,
    start_from_oldest: bool,
    shard_closed: bool,
    is_child: bool,
) -> ShardIteratorChoice {
    if !sequence.is_empty() && !is_shard_end(sequence) {
        ShardIteratorChoice::AfterSequenceNumber
    } else if shard_closed || is_child || start_from_oldest {
        ShardIteratorChoice::TrimHorizon
    } else {
        ShardIteratorChoice::Latest
    }
}

/// Whether the per-shard consumer should stop and checkpoint `SHARD_END`.
///
/// Finished when:
/// - `next_shard_iterator` is missing (AWS end-of-shard), or
/// - the shard is closed and this GetRecords returned no records, or
/// - `MillisBehindLatest` has only grown across consecutive empty polls
///   (`GROWING_AGE_POLLS_TO_FINISH`), even if ListShards has not yet marked
///   the shard closed (frozen iterator after a reshard).
pub fn should_finish_shard_consumer(
    next_iterator_missing: bool,
    records_empty: bool,
    shard_closed: bool,
    growing_empty_polls: u32,
) -> bool {
    if next_iterator_missing {
        return true;
    }
    if shard_closed && records_empty {
        return true;
    }
    growing_empty_polls >= GROWING_AGE_POLLS_TO_FINISH
}

/// Action after a failed `GetShardIterator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IteratorErrorAction {
    /// Shard is gone (`ResourceNotFound`); delete the DynamoDB row.
    Delete,
    /// Closed shard that cannot be iterated; write `SHARD_END` so it is not
    /// reclaimed with `LATEST` and children can start.
    ShardEnd,
    /// Open shard; release the lease, keep the existing sequence.
    Release,
}

pub fn iterator_error_action(shard_closed: bool, resource_not_found: bool) -> IteratorErrorAction {
    if resource_not_found {
        IteratorErrorAction::Delete
    } else if shard_closed {
        IteratorErrorAction::ShardEnd
    } else {
        IteratorErrorAction::Release
    }
}

/// Update the consecutive-growing counter for empty closed-shard polls.
pub fn next_growing_empty_polls(
    records_empty: bool,
    previous_millis_behind: Option<i64>,
    current_millis_behind: Option<i64>,
    previous_growing: u32,
) -> u32 {
    if !records_empty {
        return 0;
    }
    match (previous_millis_behind, current_millis_behind) {
        (Some(prev), Some(cur)) if cur > prev => previous_growing.saturating_add(1),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn open_shard_has_no_ending_sequence() {
        assert!(!is_shard_closed(None));
    }

    #[test]
    fn closed_shard_has_ending_sequence() {
        assert!(is_shard_closed(Some(
            "49671246667589458717803282320587893555896035326582658"
        )));
    }

    #[test]
    fn ending_sequence_null_string_is_open() {
        assert!(!is_shard_closed(Some("null")));
    }

    #[test]
    fn iterator_after_sequence_when_checkpoint_exists() {
        assert_eq!(
            shard_iterator_choice("4967", false, true, false),
            ShardIteratorChoice::AfterSequenceNumber
        );
        assert_eq!(
            shard_iterator_choice("4967", false, false, true),
            ShardIteratorChoice::AfterSequenceNumber
        );
    }

    #[test]
    fn iterator_never_latest_on_closed_shard() {
        assert_eq!(
            shard_iterator_choice("", false, true, false),
            ShardIteratorChoice::TrimHorizon
        );
        assert_eq!(
            shard_iterator_choice("", true, true, false),
            ShardIteratorChoice::TrimHorizon
        );
    }

    #[test]
    fn iterator_trim_horizon_on_child_even_when_not_start_from_oldest() {
        assert_eq!(
            shard_iterator_choice("", false, false, true),
            ShardIteratorChoice::TrimHorizon
        );
    }

    #[test]
    fn iterator_latest_on_open_root_shard_when_not_start_from_oldest() {
        assert_eq!(
            shard_iterator_choice("", false, false, false),
            ShardIteratorChoice::Latest
        );
    }

    #[test]
    fn iterator_trim_horizon_on_open_shard_when_start_from_oldest() {
        assert_eq!(
            shard_iterator_choice("", true, false, false),
            ShardIteratorChoice::TrimHorizon
        );
    }

    #[test]
    fn finish_when_next_iterator_missing() {
        assert!(should_finish_shard_consumer(true, false, false, 0));
    }

    #[test]
    fn finish_when_closed_and_empty_even_with_next_iterator() {
        assert!(should_finish_shard_consumer(false, true, true, 0));
    }

    #[test]
    fn do_not_finish_open_empty_with_next_iterator() {
        assert!(!should_finish_shard_consumer(false, true, false, 0));
    }

    #[test]
    fn finish_when_millis_behind_grew_enough_times() {
        assert!(!should_finish_shard_consumer(
            false,
            false,
            true,
            GROWING_AGE_POLLS_TO_FINISH - 1
        ));
        assert!(should_finish_shard_consumer(
            false,
            false,
            true,
            GROWING_AGE_POLLS_TO_FINISH
        ));
        // Frozen iterator can appear before ListShards reports the parent closed.
        assert!(should_finish_shard_consumer(
            false,
            true,
            false,
            GROWING_AGE_POLLS_TO_FINISH
        ));
    }

    #[test]
    fn iterator_error_deletes_when_not_found_shard_ends_when_closed() {
        assert_eq!(
            iterator_error_action(false, true),
            IteratorErrorAction::Delete
        );
        assert_eq!(
            iterator_error_action(true, true),
            IteratorErrorAction::Delete
        );
        assert_eq!(
            iterator_error_action(true, false),
            IteratorErrorAction::ShardEnd
        );
        assert_eq!(
            iterator_error_action(false, false),
            IteratorErrorAction::Release
        );
    }

    #[test]
    fn growing_polls_increment_only_on_empty_and_increasing_millis() {
        assert_eq!(next_growing_empty_polls(true, Some(100), Some(200), 0), 1);
        assert_eq!(next_growing_empty_polls(true, Some(200), Some(200), 1), 0);
        assert_eq!(next_growing_empty_polls(false, Some(100), Some(200), 1), 0);
        assert_eq!(next_growing_empty_polls(true, None, Some(200), 0), 0);
    }

    #[test]
    fn parent_ids_skip_empty_and_null() {
        assert_eq!(
            parent_ids(Some("shard-parent"), Some("shard-adjacent")),
            vec!["shard-parent", "shard-adjacent"]
        );
        assert!(parent_ids(None, None).is_empty());
        assert!(parent_ids(Some(""), Some("null")).is_empty());
        assert_eq!(parent_ids(Some("shard-parent"), None), vec!["shard-parent"]);
    }

    #[test]
    fn parents_completed_when_missing_or_shard_end() {
        let mut sequences = HashMap::new();
        sequences.insert("p1".into(), SHARD_END.into());
        assert!(parents_completed(&["p1".into(), "p2".into()], &sequences));
        sequences.insert("p2".into(), "4967".into());
        assert!(!parents_completed(&["p1".into(), "p2".into()], &sequences));
        assert!(parents_completed(&[], &sequences));
    }

    #[test]
    fn do_not_claim_shard_end_or_blocked_child() {
        let mut sequences = HashMap::new();
        sequences.insert("parent".into(), "4967".into());
        sequences.insert("done-parent".into(), SHARD_END.into());

        assert!(!should_claim_shard(
            true,
            true,
            Some(SHARD_END),
            &[],
            &sequences
        ));
        assert!(!should_claim_shard(
            false,
            false,
            None,
            &["parent".into()],
            &sequences
        ));
        assert!(should_claim_shard(
            false,
            false,
            None,
            &["done-parent".into()],
            &sequences
        ));
        assert!(should_claim_shard(
            false,
            false,
            None,
            &["trimmed-parent".into()],
            &sequences
        ));
        assert!(!should_claim_shard(true, false, None, &[], &sequences));
        assert!(should_claim_shard(true, true, Some(""), &[], &sequences));
    }

    #[test]
    fn delete_shard_end_only_after_children_have_checkpoints() {
        let mut sequences = HashMap::new();
        sequences.insert("parent".into(), SHARD_END.into());
        assert!(!should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &sequences
        ));
        sequences.insert("child-a".into(), "1".into());
        sequences.insert("child-b".into(), "".into());
        assert!(should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &sequences
        ));
        assert!(!should_delete_shard_end_lease(SHARD_END, &[], &sequences));
        assert!(!should_delete_shard_end_lease(
            "4967",
            &["child-a".into()],
            &sequences
        ));
    }

    #[test]
    fn leftover_ids_are_checkpoints_missing_from_list_shards() {
        let listed = HashSet::from(["open-1".into(), "open-2".into()]);
        let leftover = leftover_checkpoint_shard_ids(
            vec![
                "open-1".into(),
                "expired-parent".into(),
                "open-2".into(),
                "expired-empty".into(),
            ],
            &listed,
        );
        assert_eq!(leftover, vec!["expired-parent", "expired-empty"]);
    }

    #[test]
    fn children_map_from_parent_pointers() {
        let shards = vec![
            ("child-a".into(), vec!["parent".into()]),
            ("child-b".into(), vec!["parent".into()]),
            ("merged".into(), vec!["p1".into(), "p2".into()]),
        ];
        let map = children_by_parent(&shards);
        assert_eq!(map.get("parent").unwrap().len(), 2);
        assert_eq!(map.get("p1").unwrap(), &vec!["merged".to_string()]);
        assert_eq!(map.get("p2").unwrap(), &vec!["merged".to_string()]);
    }
}
