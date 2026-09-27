//! Decision helpers for completing closed Kinesis shards after a reshard.
//!
//! After a split/merge, parent shards are CLOSED (`EndingSequenceNumber` set). These
//! helpers follow KCL 3.0: iterator choice, parent-before-child eligibility, finishing
//! only on an authoritative end-of-shard signal, and when a `SHARD_END` lease may be
//! deleted without being created again.

use std::collections::{HashMap, HashSet};

/// KCL `ExtendedSequenceNumber.SHARD_END`. Written when a shard is fully consumed
/// so children can start, then deleted once children have real checkpoints.
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

/// KCL `BlockOnParentShardTask`.
///
/// A parent with a non-`SHARD_END` checkpoint is still in progress. A missing row is
/// unfinished only on an empty checkpoint table when `start_from_oldest` is set and
/// the parent is still listed: that is KCL's first-start path. After any lease
/// exists, a missing row means the parent was cleaned up or has expired, so children
/// may start and the closed parent is not read again.
pub fn parents_completed(
    parent_ids: &[String],
    sequences: &HashMap<String, String>,
    listed_shard_ids: &HashSet<String>,
    checkpoint_table_empty: bool,
    start_from_oldest: bool,
) -> bool {
    parent_ids.iter().all(|parent| match sequences.get(parent) {
        Some(seq) => is_shard_end(seq),
        None => !(checkpoint_table_empty && start_from_oldest && listed_shard_ids.contains(parent)),
    })
}

/// Whether this shard may be leased this cycle (KCL 3.0 eligibility).
///
/// Never claim `SHARD_END`. Never claim a child until its parents are completed.
/// A closed shard with no row is claimed only when the checkpoint table is empty and
/// `start_from_oldest` is true. On a non-empty table that missing row means the lease
/// was already deleted after completion.
pub fn should_claim_shard(
    shard_closed: bool,
    has_checkpoint: bool,
    sequence: Option<&str>,
    parent_ids: &[String],
    sequences: &HashMap<String, String>,
    listed_shard_ids: &HashSet<String>,
    checkpoint_table_empty: bool,
    start_from_oldest: bool,
) -> bool {
    if sequence.map(is_shard_end).unwrap_or(false) {
        return false;
    }
    if !parents_completed(
        parent_ids,
        sequences,
        listed_shard_ids,
        checkpoint_table_empty,
        start_from_oldest,
    ) {
        return false;
    }
    if shard_closed && !has_checkpoint && !(checkpoint_table_empty && start_from_oldest) {
        return false;
    }
    true
}

/// KCL `LeaseCleanupManager.cleanupLeaseForCompletedShard`.
///
/// Delete a `SHARD_END` row only when every child has a real sequence checkpoint
/// (not an empty just-claimed row) and this shard's own parents are already gone
/// from the table. An empty child list means children have not appeared yet.
pub fn should_delete_shard_end_lease(
    sequence: &str,
    child_ids: &[String],
    parent_ids: &[String],
    sequences: &HashMap<String, String>,
) -> bool {
    if !is_shard_end(sequence) || child_ids.is_empty() {
        return false;
    }
    if parent_ids
        .iter()
        .any(|parent| sequences.contains_key(parent))
    {
        return false;
    }
    child_ids
        .iter()
        .all(|child| sequences.get(child).is_some_and(|seq| !seq.is_empty()))
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
/// AWS ends a shard only when `NextShardIterator` is null, or when `ChildShards`
/// is present (returned only once the end of the shard has been reached). An empty
/// `GetRecords` page still has later records, and a rising `MillisBehindLatest`
/// means the consumer is behind, not that the shard is closed.
pub fn should_finish_shard_consumer(
    next_iterator_missing: bool,
    child_shards_present: bool,
) -> bool {
    next_iterator_missing || child_shards_present
}

/// Action after a failed `GetShardIterator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IteratorErrorAction {
    /// Shard is gone (`ResourceNotFound`); delete the DynamoDB row.
    Delete,
    /// Transient or non-terminal failure. Keep the sequence and retry on the next claim.
    Release,
}

pub fn iterator_error_action(resource_not_found: bool) -> IteratorErrorAction {
    if resource_not_found {
        IteratorErrorAction::Delete
    } else {
        IteratorErrorAction::Release
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
        assert!(should_finish_shard_consumer(true, false));
    }

    #[test]
    fn finish_when_child_shards_present() {
        assert!(should_finish_shard_consumer(false, true));
    }

    #[test]
    fn do_not_finish_on_empty_page_with_next_iterator() {
        assert!(!should_finish_shard_consumer(false, false));
    }

    #[test]
    fn iterator_error_deletes_only_when_shard_is_gone() {
        assert_eq!(iterator_error_action(true), IteratorErrorAction::Delete);
        assert_eq!(iterator_error_action(false), IteratorErrorAction::Release);
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

    fn listed(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn parents_completed_when_shard_end_or_cleaned_up() {
        let mut sequences = HashMap::new();
        sequences.insert("p1".into(), SHARD_END.into());
        let listed = listed(&["p1", "p2"]);
        assert!(parents_completed(
            &["p1".into(), "p2".into()],
            &sequences,
            &listed,
            false,
            true
        ));
        sequences.insert("p2".into(), "4967".into());
        assert!(!parents_completed(
            &["p1".into(), "p2".into()],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(parents_completed(&[], &sequences, &listed, false, true));
    }

    #[test]
    fn empty_table_blocks_children_of_listed_parents_when_starting_from_oldest() {
        let sequences = HashMap::new();
        let listed = listed(&["parent"]);
        assert!(!parents_completed(
            &["parent".into()],
            &sequences,
            &listed,
            true,
            true
        ));
        assert!(parents_completed(
            &["parent".into()],
            &sequences,
            &listed,
            true,
            false
        ));
        assert!(parents_completed(
            &["parent".into()],
            &sequences,
            &listed,
            false,
            true
        ));
    }

    #[test]
    fn do_not_claim_shard_end_or_blocked_child() {
        let mut sequences = HashMap::new();
        sequences.insert("parent".into(), "4967".into());
        sequences.insert("done-parent".into(), SHARD_END.into());
        let listed = listed(&["parent", "done-parent", "trimmed-parent", "closed"]);

        assert!(!should_claim_shard(
            true,
            true,
            Some(SHARD_END),
            &[],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(!should_claim_shard(
            false,
            false,
            None,
            &["parent".into()],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(should_claim_shard(
            false,
            false,
            None,
            &["done-parent".into()],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(should_claim_shard(
            false,
            false,
            None,
            &["trimmed-parent".into()],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(!should_claim_shard(
            true,
            false,
            None,
            &[],
            &sequences,
            &listed,
            false,
            true
        ));
        assert!(should_claim_shard(
            true,
            true,
            Some(""),
            &[],
            &sequences,
            &listed,
            false,
            true
        ));
    }

    #[test]
    fn empty_table_claims_closed_shards_only_when_starting_from_oldest() {
        let sequences = HashMap::new();
        let listed = listed(&["closed-parent"]);
        assert!(should_claim_shard(
            true,
            false,
            None,
            &[],
            &sequences,
            &listed,
            true,
            true
        ));
        assert!(!should_claim_shard(
            true,
            false,
            None,
            &[],
            &sequences,
            &listed,
            true,
            false
        ));
        assert!(!should_claim_shard(
            true,
            false,
            None,
            &[],
            &sequences,
            &listed,
            false,
            true
        ));
    }

    #[test]
    fn delete_shard_end_only_after_children_have_real_checkpoints() {
        let mut sequences = HashMap::new();
        sequences.insert("parent".into(), SHARD_END.into());
        assert!(!should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &[],
            &sequences
        ));
        sequences.insert("child-a".into(), "1".into());
        sequences.insert("child-b".into(), "".into());
        assert!(!should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &[],
            &sequences
        ));
        sequences.insert("child-b".into(), "2".into());
        assert!(should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &[],
            &sequences
        ));
        sequences.insert("grandparent".into(), SHARD_END.into());
        assert!(!should_delete_shard_end_lease(
            SHARD_END,
            &["child-a".into(), "child-b".into()],
            &["grandparent".into()],
            &sequences
        ));
        assert!(!should_delete_shard_end_lease(
            SHARD_END,
            &[],
            &[],
            &sequences
        ));
        assert!(!should_delete_shard_end_lease(
            "4967",
            &["child-a".into()],
            &[],
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
