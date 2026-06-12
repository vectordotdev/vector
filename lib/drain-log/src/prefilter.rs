use smallvec::SmallVec;
use std::collections::HashMap;

use crate::tree::Cluster;
use crate::{BucketBackend, ClusterId, StringInterner, TokenId};

/// Packs two token IDs into a single 64-bit key for the first-last prefilter index.
/// Layout: lower 32 bits = first token ID, upper 32 bits = last token ID.
/// This replaces the raw bit-manipulation `(first << 32) | (last & 0xFFFFFFFF)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FirstLastKey(u64);

impl FirstLastKey {
    /// Create a FirstLastKey from two token ID values (u64).
    pub fn from_token_ids(first: u64, last: u64) -> Self {
        FirstLastKey((first & 0xFFFFFFFF) | (last << 32))
    }

    /// Return the packed u64 value for use as a lookup key.
    pub fn pack(&self) -> u64 {
        self.0
    }
}

/** Bucket of cluster ids indexed by first / last token for a single token-count
 *  length. Built once after training, read-only during matching. */
#[derive(Debug, Default, Clone)]
pub struct PrefilterBucket {
    pub any: Vec<ClusterId>,
    pub first_keys: Vec<TokenId>,
    pub first_vals: Vec<Vec<ClusterId>>,
    pub last_keys: Vec<TokenId>,
    pub last_vals: Vec<Vec<ClusterId>>,
    pub fl_keys: Vec<TokenId>,
    pub fl_vals: Vec<Vec<ClusterId>>,
}

/** Rebuild prefilter buckets from the current set of clusters.
 *
 *  Called automatically by [`Matcher::finalize_training`][crate::Matcher]. */
pub fn rebuild_match_prefilter(
    clusters: &[Option<Cluster>],
    param_id: TokenId,
) -> Vec<PrefilterBucket> {
    let mut any_by_tc: HashMap<usize, Vec<ClusterId>> = HashMap::new();
    let mut first_by_tc: HashMap<usize, HashMap<TokenId, Vec<ClusterId>>> = HashMap::new();
    let mut last_by_tc: HashMap<usize, HashMap<TokenId, Vec<ClusterId>>> = HashMap::new();
    let mut fl_by_tc: HashMap<usize, HashMap<TokenId, Vec<ClusterId>>> = HashMap::new();
    let mut max_len = 0usize;

    for (id, cluster) in clusters.iter().enumerate().skip(1) {
        let Some(c) = cluster.as_ref() else {
            continue;
        };

        let token_count = c.token_ids.len();
        if token_count > max_len {
            max_len = token_count;
        }
        if token_count == 0 {
            any_by_tc.entry(0).or_default().push(ClusterId(id));
            continue;
        }

        let first_id = c.token_ids[0];
        let last_id = c.token_ids[token_count - 1];
        let first_is_param = first_id == param_id;
        let last_is_param = last_id == param_id;

        match (first_is_param, last_is_param) {
            (true, true) => {
                any_by_tc
                    .entry(token_count)
                    .or_default()
                    .push(ClusterId(id));
            }
            (false, true) => {
                first_by_tc
                    .entry(token_count)
                    .or_default()
                    .entry(first_id)
                    .or_default()
                    .push(ClusterId(id));
            }
            (true, false) => {
                last_by_tc
                    .entry(token_count)
                    .or_default()
                    .entry(last_id)
                    .or_default()
                    .push(ClusterId(id));
            }
            (false, false) => {
                let combined = TokenId(FirstLastKey::from_token_ids(first_id.0, last_id.0).pack());
                fl_by_tc
                    .entry(token_count)
                    .or_default()
                    .entry(combined)
                    .or_default()
                    .push(ClusterId(id));
            }
        }
    }

    let mut buckets = vec![PrefilterBucket::default(); max_len + 1];
    for (tc, ids) in any_by_tc {
        buckets[tc].any = ids;
    }
    for (tc, mm) in first_by_tc {
        let (keys, vals) = sorted_token_id_keys(mm);
        buckets[tc].first_keys = keys;
        buckets[tc].first_vals = vals;
    }
    for (tc, mm) in last_by_tc {
        let (keys, vals) = sorted_token_id_keys(mm);
        buckets[tc].last_keys = keys;
        buckets[tc].last_vals = vals;
    }
    for (tc, mm) in fl_by_tc {
        let (keys, vals) = sorted_token_id_keys(mm);
        buckets[tc].fl_keys = keys;
        buckets[tc].fl_vals = vals;
    }

    buckets
}

/** Look up candidate cluster ids for a tokenized line using first/last token
 *  indexes. Returns `None` when no candidates exist. */
pub fn prefilter_candidates_compact<'a>(
    buckets: &'a [PrefilterBucket],
    interner: &'a StringInterner<BucketBackend<usize>>,
    param_id: TokenId,
    tokens: &[std::sync::Arc<str>],
    dst: &mut SmallVec<[ClusterId; 16]>,
) -> Option<()> {
    let tc = tokens.len();
    let b = buckets.get(tc)?;

    // Fast path: no tokens → only "any" bucket applies
    if tc == 0 {
        return merge_prefilter_groups(&b.any[..], &[], &[], &[], dst);
    }

    let first_id = interner
        .get(tokens[0].as_ref())
        .map(TokenId::from)
        .unwrap_or(param_id);
    let last_id = interner
        .get(tokens[tc - 1].as_ref())
        .map(TokenId::from)
        .unwrap_or(param_id);
    let first_known = first_id != param_id;
    let last_known = last_id != param_id;

    let first = if first_known {
        search_sorted_token_id(&b.first_keys, &b.first_vals, first_id)
    } else {
        &[]
    };
    let last = if last_known {
        search_sorted_token_id(&b.last_keys, &b.last_vals, last_id)
    } else {
        &[]
    };
    let first_last = if first_known && last_known {
        let combined = TokenId(FirstLastKey::from_token_ids(first_id.0, last_id.0).pack());
        search_sorted_token_id(&b.fl_keys, &b.fl_vals, combined)
    } else {
        &[]
    };

    merge_prefilter_groups(&b.any[..], first, last, first_last, dst)
}

fn merge_prefilter_groups(
    any: &[ClusterId],
    first: &[ClusterId],
    last: &[ClusterId],
    first_last: &[ClusterId],
    dst: &mut SmallVec<[ClusterId; 16]>,
) -> Option<()> {
    let groups: [&[ClusterId]; 4] = [any, first, last, first_last];
    let non_empty = groups.iter().filter(|g| !g.is_empty()).count();
    if non_empty == 0 {
        return None;
    }
    if non_empty == 1 {
        dst.clear();
        let group = groups.into_iter().find(|g| !g.is_empty()).unwrap();
        dst.extend_from_slice(group);
        return Some(());
    }
    dst.clear();
    dst.reserve(any.len() + first.len() + last.len() + first_last.len());
    dst.extend_from_slice(any);
    dst.extend_from_slice(first);
    dst.extend_from_slice(last);
    dst.extend_from_slice(first_last);
    Some(())
}

fn search_sorted_token_id<'a>(
    keys: &'a [TokenId],
    vals: &'a [Vec<ClusterId>],
    target: TokenId,
) -> &'a [ClusterId] {
    keys.binary_search(&target)
        .map(|i| &vals[i][..])
        .unwrap_or(&[])
}

fn sorted_token_id_keys(
    m: HashMap<TokenId, Vec<ClusterId>>,
) -> (Vec<TokenId>, Vec<Vec<ClusterId>>) {
    let mut items: Vec<(TokenId, Vec<ClusterId>)> = m.into_iter().collect();
    items.sort_unstable_by_key(|(k, _)| *k);
    items.into_iter().unzip()
}
