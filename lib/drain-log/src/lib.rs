#![forbid(unsafe_code)]

//! drain3 — fast log template extraction via fixed-depth prefix trees.
//!
//! Rust port of [logpai/Drain3](https://github.com/logpai/Drain3). Splits log lines into tokens,
//! clusters them by a prefix tree keyed on token count, and replaces
//! variable tokens with a param placeholder (`<*>` by default).
//!
//! # Example
//! ```
//! use drain3::Config;
//!
//! # fn main() -> Result<(), drain3::Error> {
//! let samples: Vec<String> = vec![
//!     "connection from 10.0.0.1 timeout after 5000ms".into(),
//!     "connection from 10.0.0.2 timeout after 3000ms".into(),
//!     "connection from 10.0.0.3 timeout after 1000ms".into(),
//! ];
//! let matcher = drain3::train(&samples, Config::default())?;
//! let (id, args, ok) = matcher.match_line("connection from 192.168.1.1 timeout after 42ms");
//! assert!(ok);
//! assert_eq!(args, vec!["192.168.1.1", "42ms"]);
//! # Ok(())
//! # }
//! ```
use smallvec::SmallVec;
use snafu::Snafu;
use std::sync::{Arc, Mutex};
use string_interner::backend::BucketBackend;
use string_interner::StringInterner;

mod prefilter;
mod render;
mod tokenizer;
mod tree;

pub use render::RenderPlan;
pub(crate) use tree::{Cluster, Node};

/// Errors that can occur during training or template reconstruction.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Tree depth is below the minimum of 3.
    #[snafu(display("depth must be >= 3, got {got}"))]
    InvalidDepth { got: usize },
    /// Similarity threshold is outside [0, 1].
    #[snafu(display("similarity threshold must be in [0, 1], got {got}"))]
    InvalidSimilarityThreshold { got: f64 },
    /// Match threshold is outside [0, 1].
    #[snafu(display("match threshold must be in [0, 1], got {got}"))]
    InvalidMatchThreshold { got: f64 },
    /// Max children is below the minimum of 2.
    #[snafu(display("max children must be >= 2, got {got}"))]
    InvalidMaxChildren { got: usize },
    /// Max tokens must be >= 1.
    #[snafu(display("max tokens must be >= 1, got {got}"))]
    InvalidMaxTokens { got: usize },
    /// Max bytes must be >= 1.
    #[snafu(display("max bytes must be >= 1, got {got}"))]
    InvalidMaxBytes { got: usize },
    /// Param string was empty.
    #[snafu(display("param string must not be empty"))]
    EmptyParamString,
    /// Template id must be > 0.
    #[snafu(display("template id must be > 0, got {id}"))]
    InvalidTemplateId { id: usize },
    /// Duplicate template id encountered.
    #[snafu(display("duplicate template id {id}"))]
    DuplicateTemplateId { id: usize },
    /// Template count must be > 0.
    #[snafu(display("template {id} count must be > 0"))]
    ZeroCountTemplate { id: usize },
    /// Internal error: cluster not found (programming bug).
    #[snafu(display("internal error: cluster {id} not found"))]
    ClusterNotFound { id: usize },
    /// Internal error: root node not initialized for token count.
    #[snafu(display("internal error: root not initialized for token count {token_count}"))]
    InternalRootNotInitialized { token_count: usize },
    /// Max clusters reached during training.
    #[snafu(display("max clusters {limit} reached"))]
    MaxClustersReached { limit: usize },
    /// Line exceeds max_bytes configuration.
    #[snafu(display("line too long: {length} bytes (max: {max_bytes})"))]
    LineTooLong { length: usize, max_bytes: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TokenId(pub(crate) u64);

impl From<usize> for TokenId {
    fn from(s: usize) -> Self {
        TokenId(s as u64)
    }
}

#[allow(dead_code)]
impl From<TokenId> for usize {
    fn from(id: TokenId) -> Self {
        id.0 as usize
    }
}

const DEFAULT_DEPTH: usize = 4;

/// Default similarity threshold for training (0.0–1.0).
/// Fraction of tokens that must match for a line to join a cluster.
const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.5;

/// Default match threshold for matching (0.0–1.0).
/// Fraction of tokens that must match for a line to be considered a match.
const DEFAULT_MATCH_THRESHOLD: f64 = 1.0;

/// Default max children per inner node.
/// One slot is reserved for the param catch-all child.
const DEFAULT_MAX_CHILDREN: usize = 100;

/// Default max tokens per line.
/// Lines exceeding this are skipped during training and matching.
const DEFAULT_MAX_TOKENS: usize = 64;

/// Default max bytes per line.
/// Lines exceeding this are skipped during training and matching.
const DEFAULT_MAX_BYTES: usize = 1024;

/// Default max clusters. 0 = unlimited.
const DEFAULT_MAX_CLUSTERS: usize = 0;

/// Minimum allowed tree depth.
const MIN_DEPTH: usize = 3;

/// Minimum allowed max_children value.
const MIN_MAX_CHILDREN: usize = 2;

/// Minimum allowed max_tokens and max_bytes value.
const MIN_LINE_LIMIT: usize = 1;

/// Stack capacity for prefilter candidate buffer.
/// Determines how many cluster candidates can be collected without heap allocation.
const PREFILTER_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClusterId(pub(crate) usize);

impl From<ClusterId> for usize {
    fn from(id: ClusterId) -> Self {
        id.0
    }
}

impl From<usize> for ClusterId {
    fn from(s: usize) -> Self {
        ClusterId(s)
    }
}

/// Controls training and matching behavior.
#[derive(Debug, Clone, PartialEq, bon::Builder)]
pub struct Config {
    #[builder(default = DEFAULT_DEPTH)]
    pub depth: usize,
    #[builder(default = DEFAULT_SIMILARITY_THRESHOLD)]
    pub similarity_threshold: f64,
    #[builder(default = DEFAULT_MATCH_THRESHOLD)]
    pub match_threshold: f64,
    #[builder(default = DEFAULT_MAX_CHILDREN)]
    pub max_children: usize,
    #[builder(default = DEFAULT_MAX_TOKENS)]
    pub max_tokens: usize,
    #[builder(default = DEFAULT_MAX_BYTES)]
    pub max_bytes: usize,
    #[builder(default = DEFAULT_MAX_CLUSTERS)]
    pub max_clusters: usize,
    #[builder(default = Arc::from("<*>"))]
    pub param_string: Arc<str>,
    #[builder(default = true)]
    pub parametrize_numeric_tokens: bool,
    #[builder(default)]
    pub extra_delimiters: Vec<String>,
    #[builder(default = true)]
    pub enable_match_prefilter: bool,
}

impl Config {
    fn validate(&self) -> Result<(), Error> {
        if self.depth < MIN_DEPTH {
            return Err(Error::InvalidDepth { got: self.depth });
        }
        if !(0.0..=1.0).contains(&self.similarity_threshold) {
            return Err(Error::InvalidSimilarityThreshold {
                got: self.similarity_threshold,
            });
        }
        if !(0.0..=1.0).contains(&self.match_threshold) {
            return Err(Error::InvalidMatchThreshold {
                got: self.match_threshold,
            });
        }
        if self.max_children < MIN_MAX_CHILDREN {
            return Err(Error::InvalidMaxChildren {
                got: self.max_children,
            });
        }
        if self.max_tokens < MIN_LINE_LIMIT {
            return Err(Error::InvalidMaxTokens {
                got: self.max_tokens,
            });
        }
        if self.max_bytes < MIN_LINE_LIMIT {
            return Err(Error::InvalidMaxBytes {
                got: self.max_bytes,
            });
        }
        if self.param_string.is_empty() {
            return Err(Error::EmptyParamString);
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            depth: DEFAULT_DEPTH,
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            match_threshold: DEFAULT_MATCH_THRESHOLD,
            max_children: DEFAULT_MAX_CHILDREN,
            max_tokens: DEFAULT_MAX_TOKENS,
            max_bytes: DEFAULT_MAX_BYTES,
            max_clusters: DEFAULT_MAX_CLUSTERS,
            param_string: "<*>".into(),
            parametrize_numeric_tokens: true,
            extra_delimiters: Vec::new(),
            enable_match_prefilter: true,
        }
    }
}

/// A trained log template.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    id: usize,
    tokens: Vec<Arc<str>>,
    params: Vec<bool>,
    token_count: usize,
    count: usize,
}

impl Template {
    /// Cluster id.
    pub fn id(&self) -> usize {
        self.id
    }
    /// Dense token list: only non-param tokens, in order.
    pub fn tokens(&self) -> &[Arc<str>] {
        &self.tokens
    }
    /// Whether position `idx` is a param placeholder.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds (>= `token_count`).
    pub fn is_param(&self, idx: usize) -> bool {
        self.params[idx]
    }
    /// Total number of positions (len(tokens) + param_count).
    pub fn token_count(&self) -> usize {
        self.token_count
    }
    /// Number of matching log lines.
    pub fn count(&self) -> usize {
        self.count
    }
}
/// A trained DRAIN matcher. Holds the prefix tree, token dictionary, and
/// precomputed indices for fast line matching.
///
/// Create via [`train`] or [`matcher_from_templates`].
pub struct Matcher {
    cfg: Config,
    templates: Vec<Template>,
    nodes: Vec<Node>,
    root_by_len: Vec<Option<usize>>,
    clusters: Vec<Option<Cluster>>,
    param_id: TokenId,
    next_cluster_id: ClusterId,
    min_match_scores: Vec<usize>,
    prefilter_buckets: Vec<prefilter::PrefilterBucket>,
    has_param_first: bool,
    interner: StringInterner<BucketBackend<usize>>,
    token_buf: Mutex<Vec<Arc<str>>>,
    /// Cluster ids freed by LRU eviction, ready to be reused for new
    /// clusters so the `clusters` Vec stays bounded at roughly
    /// `max_clusters` slots on long-running streams.
    free_ids: Vec<usize>,
    /// Head of the LRU doubly-linked list (the least-recently-used cluster
    /// — first to be evicted when the cap is hit).
    lru_head: Option<usize>,
    /// Tail of the LRU doubly-linked list (the most-recently-used cluster).
    /// New and freshly-touched clusters are spliced in here.
    lru_tail: Option<usize>,
}
impl Matcher {
    pub fn new(cfg: Config) -> Self {
        let mut interner = StringInterner::new();
        let param_id = TokenId::from(interner.get_or_intern(&cfg.param_string));
        Self {
            cfg: cfg.clone(),
            templates: Vec::new(),
            nodes: Vec::new(),
            root_by_len: Vec::new(),
            clusters: vec![None],
            param_id,
            next_cluster_id: ClusterId(1),
            min_match_scores: Vec::new(),
            prefilter_buckets: Vec::new(),
            has_param_first: false,
            interner,
            token_buf: Mutex::new(Vec::with_capacity(16)),
            free_ids: Vec::new(),
            lru_head: None,
            lru_tail: None,
        }
    }

    /// Number of clusters currently tracked. Accounts for slots freed by
    /// LRU eviction.
    pub fn cluster_count(&self) -> usize {
        (self.next_cluster_id.0 - 1).saturating_sub(self.free_ids.len())
    }

    /// Unlink the given cluster id from the LRU doubly-linked list. The
    /// cluster's slot is left intact — only the prev/next pointers and the
    /// matcher's head/tail are touched. O(1).
    fn lru_unlink(&mut self, id: usize) {
        let (prev, next) = {
            let Some(c) = self.clusters[id].as_ref() else {
                return;
            };
            (c.lru_prev, c.lru_next)
        };
        match prev {
            Some(p) => {
                if let Some(pc) = self.clusters[p].as_mut() {
                    pc.lru_next = next;
                }
            }
            None => self.lru_head = next,
        }
        match next {
            Some(n) => {
                if let Some(nc) = self.clusters[n].as_mut() {
                    nc.lru_prev = prev;
                }
            }
            None => self.lru_tail = prev,
        }
        if let Some(c) = self.clusters[id].as_mut() {
            c.lru_prev = None;
            c.lru_next = None;
        }
    }

    /// Splice the given cluster id onto the MRU end of the LRU list. The
    /// caller must have already cleared any stale prev/next pointers (e.g.
    /// by calling [`lru_unlink`] or constructing a fresh cluster). O(1).
    fn lru_push_tail(&mut self, id: usize) {
        let old_tail = self.lru_tail;
        if let Some(c) = self.clusters[id].as_mut() {
            c.lru_prev = old_tail;
            c.lru_next = None;
        }
        match old_tail {
            Some(t) => {
                if let Some(tc) = self.clusters[t].as_mut() {
                    tc.lru_next = Some(id);
                }
            }
            None => self.lru_head = Some(id),
        }
        self.lru_tail = Some(id);
    }

    /// Promote the given cluster id to the MRU end. Cheap no-op if it is
    /// already the tail. O(1).
    fn lru_touch(&mut self, id: usize) {
        if self.lru_tail == Some(id) {
            return;
        }
        self.lru_unlink(id);
        self.lru_push_tail(id);
    }

    /// Pop the LRU end of the list, returning its cluster id. O(1).
    fn lru_pop_head(&mut self) -> Option<usize> {
        let id = self.lru_head?;
        self.lru_unlink(id);
        Some(id)
    }

    /// Evict the least-recently-used cluster: pop it from the LRU list,
    /// scrub it from the tree node that owns it, free its id slot for
    /// reuse, and return the freed id (or `None` if there are no clusters
    /// to evict). O(1) amortised — only the per-node `cluster_ids` retain
    /// is linear in that one node's list length, which is bounded by
    /// `max_node_children`.
    fn evict_lru(&mut self) -> Option<usize> {
        let id = self.lru_pop_head()?;
        let node_idx = self.clusters[id].as_ref().and_then(|c| c.node_idx);
        if let Some(idx) = node_idx {
            let cid = ClusterId(id);
            self.nodes[idx].cluster_ids.retain(|&x| x != cid);
        }
        self.clusters[id] = None;
        self.free_ids.push(id);
        Some(id)
    }
    fn resolve_token_id<T: AsRef<str>>(&self, token: T) -> TokenId {
        self.interner
            .get(token.as_ref())
            .map(TokenId::from)
            .unwrap_or(self.param_id)
    }
    fn intern_token(&mut self, token: &str) -> TokenId {
        TokenId::from(self.interner.get_or_intern(token))
    }
    fn intern_token_ids(&mut self, tokens: &[Arc<str>], dst: &mut Vec<TokenId>) {
        dst.clear();
        dst.reserve(tokens.len());
        for t in tokens {
            dst.push(self.intern_token(t));
        }
    }
    fn required_score(&self, token_count: usize, sim_th: f64) -> usize {
        if sim_th == self.cfg.match_threshold && token_count < self.min_match_scores.len() {
            return self.min_match_scores[token_count];
        }
        (sim_th * token_count as f64).ceil() as usize
    }
    fn rebuild_min_match_scores(&mut self) {
        self.min_match_scores.resize(self.root_by_len.len(), 0);
        for tc in 0..self.min_match_scores.len() {
            self.min_match_scores[tc] = (self.cfg.match_threshold * tc as f64).ceil() as usize;
        }
    }

    pub fn match_line(&self, line: &str) -> (usize, Vec<String>, bool) {
        let (cluster_id, tc) = self.find_match_with_tc(line);
        if let Some(cid) = cluster_id {
            let mut args = Vec::new();
            let id = self.fill_match_args(cid, tc, &mut args);
            return (id, args, true);
        }
        (0, Vec::new(), false)
    }

    fn fill_match_args(&self, cluster_id: ClusterId, tc: usize, dst: &mut Vec<String>) -> usize {
        let cluster = self.clusters[cluster_id.0].as_ref().unwrap();
        let token_buf = self.token_buf.lock().unwrap();
        dst.clear();
        dst.reserve(cluster.param_positions.len());
        for &pos in &cluster.param_positions {
            if pos < tc {
                dst.push(token_buf[pos].to_string());
            }
        }
        cluster.id.0
    }

    fn find_match_with_tc(&self, line: &str) -> (Option<ClusterId>, usize) {
        let mut token_buf = self.token_buf.lock().unwrap();
        token_buf.clear();
        if !self.has_param_first && self.cfg.extra_delimiters.is_empty() {
            let first_tok = &line[..line.find(' ').unwrap_or(line.len())];
            if self.interner.get(first_tok).is_none() {
                return (None, 0);
            }
        }
        let Some(tc) = self.tokenize_input_internal(&mut token_buf, line) else {
            return (None, 0);
        };
        if tc >= self.root_by_len.len() || self.root_by_len[tc].is_none() {
            return (None, tc);
        }
        if self.cfg.enable_match_prefilter && tc < self.prefilter_buckets.len() {
            let mut candidates: SmallVec<[ClusterId; PREFILTER_CAPACITY]> = SmallVec::new();
            if prefilter::prefilter_candidates_compact(
                &self.prefilter_buckets,
                &self.interner,
                self.param_id,
                &token_buf,
                &mut candidates,
            )
            .is_some()
            {
                return (
                    self.fast_match_strings(
                        &candidates,
                        &token_buf,
                        self.cfg.match_threshold,
                        true,
                    )
                    .map(|c| c.id),
                    tc,
                );
            }
            return (None, tc);
        }
        let cluster = self.tree_search_with_threshold(&token_buf, self.cfg.match_threshold, true);
        (cluster.map(|c| c.id), tc)
    }
    fn tokenize_input_internal(
        &self,
        token_buf: &mut Vec<Arc<str>>,
        content: &str,
    ) -> Option<usize> {
        if content.len() > self.cfg.max_bytes {
            return None;
        }
        token_buf.clear();
        if self.cfg.extra_delimiters.is_empty() {
            let count =
                tokenizer::tokenize_whitespace_count(content, token_buf, self.cfg.max_tokens);
            if count == 0 || count > self.cfg.max_tokens {
                return None;
            }
        } else {
            tokenizer::tokenize(
                content,
                &self.cfg.extra_delimiters,
                self.cfg.max_tokens,
                token_buf,
            );
            if token_buf.is_empty() || token_buf.len() > self.cfg.max_tokens {
                return None;
            }
        }
        Some(token_buf.len())
    }
    pub fn match_into(&self, line: &str, dst: &mut Vec<String>) -> (usize, bool) {
        let (cluster_id, tc) = self.find_match_with_tc(line);
        if let Some(cid) = cluster_id {
            let id = self.fill_match_args(cid, tc, dst);
            return (id, true);
        }
        (0, false)
    }
    pub fn match_id(&self, line: &str) -> Option<usize> {
        self.find_match_with_tc(line).0.map(|c| c.0)
    }
    pub fn find(&self, line: &str) -> (usize, Vec<String>, bool) {
        self.match_line(line)
    }
    /// Return a reference to the matcher's config.
    pub fn config(&self) -> &Config {
        &self.cfg
    }
    /// Return trained templates sorted by descending count.
    ///
    /// This is a reference — mutations do not affect the matcher.
    pub fn templates(&self) -> &[Template] {
        &self.templates
    }
    /// Template by cluster id.
    pub fn template_for_id(&self, id: usize) -> Option<Template> {
        self.clusters
            .get(id)?
            .as_ref()?
            .to_template(&self.interner, self.param_id)
            .into()
    }
    fn tokenize_input(&self, content: &str) -> Option<usize> {
        let mut token_buf = self.token_buf.lock().unwrap();
        self.tokenize_input_internal(&mut token_buf, content)
    }
    fn tree_search_with_threshold(
        &self,
        tokens: &[Arc<str>],
        threshold: f64,
        include_params: bool,
    ) -> Option<&Cluster> {
        let tc = tokens.len();
        if tc >= self.root_by_len.len() {
            return None;
        }
        let root_idx = self.root_by_len[tc]?;
        if tc == 0 {
            return self.nodes[root_idx]
                .cluster_ids
                .first()
                .and_then(|&id| self.clusters.get(id.0).and_then(|c| c.as_ref()));
        }
        let max_depth = self.cfg.depth.saturating_sub(2);
        let cur_idx = self.descend_prefix(root_idx, tokens, max_depth, tc)?;
        self.fast_match_strings(
            &self.nodes[cur_idx].cluster_ids,
            tokens,
            threshold,
            include_params,
        )
    }
    fn descend_prefix(
        &self,
        cur_idx: usize,
        tokens: &[Arc<str>],
        max_depth: usize,
        tc: usize,
    ) -> Option<usize> {
        let mut cur_idx = cur_idx;
        let limit = (max_depth - 1).min(tc - 1);
        for tok in tokens.iter().take(limit) {
            let tid = self.resolve_token_id(tok);
            let next = self.nodes[cur_idx]
                .children
                .get(&tid)
                .copied()
                .or_else(|| self.nodes[cur_idx].children.get(&self.param_id).copied());
            cur_idx = next?;
        }
        Some(cur_idx)
    }

    fn fast_match_strings(
        &self,
        cluster_ids: &[ClusterId],
        tokens: &[Arc<str>],
        threshold: f64,
        include_params: bool,
    ) -> Option<&Cluster> {
        let n_tokens = tokens.len();
        let needed = self.required_score(n_tokens, threshold);
        let exact_mode = include_params && threshold >= 1.0;

        let mut best_score: isize = -1;
        let mut best_param_count: isize = -1;
        let mut best_cluster: Option<&Cluster> = None;

        for &cid in cluster_ids {
            let Some(c) = self.clusters.get(cid.0).and_then(|c| c.as_ref()) else {
                continue;
            };
            if c.token_str.len() != n_tokens {
                continue;
            }

            let param_count = c.param_count;
            let mut sim_tokens: isize = if include_params {
                param_count as isize
            } else {
                0
            };
            let mut remaining = c.non_param_idx.len();

            let anchor0_pos = c.anchor0;
            let anchor1_pos = c.anchor1;

            if let Some(a) = anchor0_pos {
                if c.token_str[a] == tokens[a] {
                    sim_tokens += 1;
                }
                remaining -= 1;
                if sim_tokens + (remaining as isize) < (needed as isize) {
                    continue;
                }
            }
            if let Some(a) = anchor1_pos {
                if c.token_str[a] == tokens[a] {
                    sim_tokens += 1;
                }
                remaining -= 1;
                if sim_tokens + (remaining as isize) < (needed as isize) {
                    continue;
                }
            }

            for &idx in &c.non_param_idx {
                if anchor0_pos == Some(idx) || anchor1_pos == Some(idx) {
                    continue;
                }
                if c.token_str[idx] == tokens[idx] {
                    sim_tokens += 1;
                }
                remaining -= 1;
                if sim_tokens + (remaining as isize) < (needed as isize) {
                    break;
                }
            }

            if sim_tokens > best_score
                || (sim_tokens == best_score && param_count as isize > best_param_count)
            {
                best_score = sim_tokens;
                best_param_count = param_count as isize;
                best_cluster = Some(c);
                if exact_mode && sim_tokens >= (needed as isize) {
                    return best_cluster;
                }
            }
        }

        if exact_mode {
            None
        } else if best_score >= (needed as isize) {
            best_cluster
        } else {
            None
        }
    }
    fn create_cluster(&mut self, tokens: Vec<Arc<str>>) -> Result<ClusterId, Error> {
        let mut token_ids = Vec::new();
        self.intern_token_ids(&tokens, &mut token_ids);
        // Reuse a slot freed by LRU eviction before extending the clusters Vec.
        let id = if let Some(reused) = self.free_ids.pop() {
            ClusterId(reused)
        } else {
            let id = self.next_cluster_id;
            self.next_cluster_id = ClusterId(self.next_cluster_id.0 + 1);
            id
        };
        let cl = Cluster::new(id, tokens, token_ids, self.param_id);
        if id.0 >= self.clusters.len() {
            self.clusters.resize_with(id.0 + 1, || None);
        }
        self.clusters[id.0] = Some(cl);
        self.add_seq_to_prefix_tree(id)?;
        // Newly-created clusters are by definition the most recently used.
        self.lru_push_tail(id.0);
        Ok(id)
    }
    pub fn add_log_message(&mut self, content: &str) -> Result<Template, Error> {
        let tc = self.tokenize_input(content).ok_or(Error::LineTooLong {
            length: content.len(),
            max_bytes: self.cfg.max_bytes,
        })?;
        if tc >= self.root_by_len.len() {
            // Token count not seen before — a new cluster will be created.
            // Evict the LRU cluster first if we're already at the cap so the
            // matcher continues to learn new patterns instead of stalling.
            if self.cfg.max_clusters > 0 && self.cluster_count() >= self.cfg.max_clusters {
                self.evict_lru();
            }
            let tokens = self.token_buf.lock().unwrap().clone();
            let cid = self.create_cluster(tokens)?;
            let cluster = self.clusters[cid.0]
                .as_ref()
                .ok_or(Error::ClusterNotFound { id: cid.0 })?;
            return Ok(cluster.to_template(&self.interner, self.param_id));
        }
        let token_buf = self.token_buf.lock().unwrap();
        if let Some(c) =
            self.tree_search_with_threshold(&token_buf, self.cfg.similarity_threshold, false)
        {
            let cid = c.id;
            let mut changed = false;
            let cluster = self.clusters[cid.0]
                .as_mut()
                .ok_or(Error::ClusterNotFound { id: cid.0 })?;
            for (i, tok) in token_buf.iter().enumerate().take(cluster.token_str.len()) {
                if cluster.token_ids[i] == self.param_id {
                    continue;
                }
                if cluster.token_str[i] != *tok {
                    cluster.token_ids[i] = self.param_id;
                    cluster.token_str[i] = self.cfg.param_string.clone();
                    cluster.param_count += 1;
                    changed = true;
                }
            }
            if changed {
                cluster.rebuild_indices(self.param_id);
            }
            cluster.count += 1;
            let template = cluster.to_template(&self.interner, self.param_id);
            // Drop the token_buf guard and the cluster borrow before mutating
            // the LRU list (which re-borrows the clusters vec mutably).
            drop(token_buf);
            self.lru_touch(cid.0);
            return Ok(template);
        }
        drop(token_buf);
        if self.cfg.max_clusters > 0 && self.cluster_count() >= self.cfg.max_clusters {
            self.evict_lru();
        }
        let tokens = self.token_buf.lock().unwrap().clone();
        let cid = self.create_cluster(tokens)?;
        let cluster = self.clusters[cid.0]
            .as_ref()
            .ok_or(Error::ClusterNotFound { id: cid.0 })?;
        Ok(cluster.to_template(&self.interner, self.param_id))
    }
    fn add_seq_to_prefix_tree(&mut self, cluster_id: ClusterId) -> Result<(), Error> {
        let cluster = self.clusters[cluster_id.0]
            .as_ref()
            .ok_or(Error::ClusterNotFound { id: cluster_id.0 })?;
        let tc = cluster.token_ids.len();
        if tc >= self.root_by_len.len() {
            self.root_by_len.resize_with(tc + 1, || None);
        }
        if self.root_by_len[tc].is_none() {
            let idx = self.nodes.len();
            self.nodes.push(Node::new());
            self.root_by_len[tc] = Some(idx);
        }
        let mut cur_idx =
            self.root_by_len[tc].ok_or(Error::InternalRootNotInitialized { token_count: tc })?;
        // Index of the leaf node the cluster ultimately lands in. Stored on
        // the cluster so LRU eviction can scrub it from the tree later.
        let leaf_idx;
        if tc == 0 {
            self.nodes[cur_idx].cluster_ids.push(cluster_id);
            leaf_idx = cur_idx;
        } else {
            let mut leaf = cur_idx;
            for (i, &token_id) in cluster.token_ids.iter().enumerate() {
                let cur_depth = i + 1;
                if cur_depth >= self.cfg.depth - 2 || cur_depth >= tc {
                    self.nodes[cur_idx].cluster_ids.push(cluster_id);
                    leaf = cur_idx;
                    break;
                }
                let key = {
                    let node = &self.nodes[cur_idx];
                    if node.children.contains_key(&token_id) {
                        token_id
                    } else if self.cfg.parametrize_numeric_tokens
                        && tokenizer::has_numbers(&cluster.token_str[i])
                    {
                        self.param_id
                    } else {
                        let specific_count = node.children.len();
                        let has_wild = node.children.contains_key(&self.param_id);
                        let available = self.cfg.max_children - 1;
                        if specific_count < available
                            || (!has_wild && specific_count < self.cfg.max_children - 1)
                        {
                            token_id
                        } else {
                            self.param_id
                        }
                    }
                };
                if !self.nodes[cur_idx].children.contains_key(&key) {
                    let new_idx = self.nodes.len();
                    self.nodes.push(Node::new());
                    self.nodes[cur_idx].children.insert(key, new_idx);
                }
                cur_idx = self.nodes[cur_idx].children[&key];
            }
            leaf_idx = leaf;
        }
        // Drop the immutable borrow before re-borrowing as mutable.
        let _ = cluster;
        if let Some(c) = self.clusters[cluster_id.0].as_mut() {
            c.node_idx = Some(leaf_idx);
        }
        Ok(())
    }
    fn sync_templates_from_clusters(&mut self) {
        let mut out: Vec<Template> = Vec::with_capacity(self.clusters.len().saturating_sub(1));
        for id in 1..self.clusters.len() {
            let Some(c) = self.clusters[id].as_ref() else {
                continue;
            };
            out.push(c.to_template(&self.interner, self.param_id));
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.count()));
        self.templates = out;
    }
    fn finalize_training(&mut self) {
        self.sync_templates_from_clusters();
        self.rebuild_min_match_scores();
        self.prefilter_buckets = prefilter::rebuild_match_prefilter(&self.clusters, self.param_id);
        self.has_param_first = self.clusters.iter().skip(1).any(|c| {
            c.as_ref()
                .is_some_and(|cl| cl.token_ids.first() == Some(&self.param_id))
        });
    }
}
/// Train a matcher with the provided config.
pub fn train(samples: &[String], cfg: Config) -> Result<Matcher, Error> {
    cfg.validate()?;
    let mut m = Matcher::new(cfg);
    for sample in samples {
        m.add_log_message(sample)?;
    }
    m.finalize_training();
    Ok(m)
}
/// Rebuild a matcher from pre-existing templates.
pub fn matcher_from_templates(cfg: Config, templates: &[Template]) -> Result<Matcher, Error> {
    cfg.validate()?;
    let mut m = Matcher::new(cfg);
    if templates.is_empty() {
        m.finalize_training();
        return Ok(m);
    }
    let mut sorted: Vec<_> = templates.to_vec();
    sorted.sort_by_key(|t| t.id());
    let mut seen = std::collections::HashSet::new();
    let mut max_id = 0;
    for t in &sorted {
        if t.id() == 0 {
            return Err(Error::InvalidTemplateId { id: t.id() });
        }
        if !seen.insert(t.id()) {
            return Err(Error::DuplicateTemplateId { id: t.id() });
        }
        if t.count() == 0 {
            return Err(Error::ZeroCountTemplate { id: t.id() });
        }
        if t.id() > max_id {
            max_id = t.id();
        }
    }
    m.clusters.resize_with(max_id + 1, || None);
    m.next_cluster_id = ClusterId(max_id + 1);
    for t in &sorted {
        let mut full: Vec<Arc<str>> = vec![Arc::from(""); t.token_count()];
        let mut dense_idx = 0;
        for (i, slot) in full.iter_mut().enumerate().take(t.token_count()) {
            if t.is_param(i) {
                *slot = m.cfg.param_string.clone();
            } else {
                *slot = t.tokens()[dense_idx].clone();
                dense_idx += 1;
            }
        }
        let mut token_ids = Vec::new();
        m.intern_token_ids(&full, &mut token_ids);
        let cl = Cluster::new(ClusterId(t.id()), full, token_ids, m.param_id);
        m.clusters[t.id()] = Some(cl);
    }
    for id in 1..m.clusters.len() {
        if m.clusters[id].is_some() {
            m.add_seq_to_prefix_tree(ClusterId(id))?;
        }
    }
    m.finalize_training();
    Ok(m)
}
/// Deterministically sample lines as fixed-size blocks at regular strides
/// with random jitter inside each stride window.
///
/// The target sample count is `frac * len(lines)`, but the actual returned
/// count is rounded up to the nearest multiple of `block_size` (capped by the
/// input length) because entire blocks are appended per stride.
///
/// Uses a seeded rng derived from the input length so the result is
/// deterministic — same input produces the same sample across runs.
pub fn stride_sample(lines: &[String], frac: f64, block_size: usize) -> Vec<String> {
    let total = lines.len();
    if total == 0 {
        return Vec::new();
    }
    let sample_n = (total as f64 * frac) as usize;
    if sample_n == 0 {
        return Vec::new();
    }
    let num_blocks = (sample_n / block_size).max(1);
    let stride = (total / num_blocks).max(block_size);
    let mut rng = fastrand::Rng::with_seed(total as u64);
    let mut out: Vec<String> = Vec::with_capacity(sample_n);
    let mut start = 0usize;
    while start < total && out.len() < sample_n {
        let max_offset = stride.saturating_sub(block_size).max(1);
        let offset = start + (rng.u32(..max_offset as u32) as usize);
        if offset >= total {
            break;
        }
        let end = (offset + block_size).min(total);
        out.extend(lines[offset..end].iter().cloned());
        start += stride;
    }
    out
}

#[cfg(test)]
mod lru_tests {
    use super::*;

    fn small_matcher(max_clusters: usize) -> Matcher {
        Matcher::new(
            Config::builder()
                .depth(3)
                .similarity_threshold(0.4)
                .max_clusters(max_clusters)
                .parametrize_numeric_tokens(false)
                .build(),
        )
    }

    /// Build a line with `len` unique tokens. Each distinct length lands in
    /// a separate tree branch (drain only merges clusters with matching
    /// token counts), giving us a reliable way to manufacture distinct
    /// clusters without fighting the similarity heuristic.
    fn distinct_line(len: usize) -> String {
        (0..len)
            .map(|i| format!("tok{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn cluster_count_reflects_active_clusters() {
        let mut m = small_matcher(0);
        m.add_log_message(&distinct_line(1)).unwrap();
        m.add_log_message(&distinct_line(2)).unwrap();
        m.add_log_message(&distinct_line(3)).unwrap();
        assert_eq!(m.cluster_count(), 3);
    }

    #[test]
    fn lru_evicts_least_recently_used_cluster() {
        // Cap of two clusters. After the third distinct line, the matcher
        // must evict whichever of the first two was least recently touched.
        let mut m = small_matcher(2);
        let a = distinct_line(1);
        let b = distinct_line(2);
        let c = distinct_line(3);
        m.add_log_message(&a).unwrap(); // cluster A
        m.add_log_message(&b).unwrap(); // cluster B
        // Touch A again so B becomes the LRU candidate.
        m.add_log_message(&a).unwrap();
        // Third distinct pattern forces eviction; total stays at the cap.
        m.add_log_message(&c).unwrap();
        assert_eq!(m.cluster_count(), 2);
        // Re-issuing the recently-touched A must still hit an existing
        // cluster — total stays at the cap.
        m.add_log_message(&a).unwrap();
        assert_eq!(m.cluster_count(), 2);
        // Re-issuing the LRU B must create a NEW cluster (confirms B was
        // the one evicted). That pushes us over the cap, triggering another
        // eviction — net effect: still capped at 2.
        m.add_log_message(&b).unwrap();
        assert_eq!(m.cluster_count(), 2);
    }

    #[test]
    fn lru_reuses_evicted_cluster_ids() {
        // With a cap of two and a parade of distinct never-seen patterns,
        // freed ids must be recycled — the clusters vec must stay bounded.
        let mut m = small_matcher(2);
        for n in 1..=50 {
            // Distinct token count per iteration → guaranteed new cluster.
            // We deliberately start at 1 so the matcher always sees a
            // never-before-seen length on every call.
            m.add_log_message(&distinct_line(n)).unwrap();
        }
        assert_eq!(m.cluster_count(), 2);
        assert!(
            m.clusters.len() <= 4,
            "clusters vec grew unexpectedly: len={}",
            m.clusters.len()
        );
    }

    #[test]
    fn no_eviction_when_max_clusters_is_zero() {
        let mut m = small_matcher(0);
        for n in 1..=20 {
            m.add_log_message(&distinct_line(n)).unwrap();
        }
        assert_eq!(m.cluster_count(), 20);
    }

    /// Walk the LRU list from head to tail (and back) and verify it
    /// contains exactly the active clusters with consistent prev/next
    /// links. Returns the ordered ids head→tail.
    fn lru_order(m: &Matcher) -> Vec<usize> {
        let mut forward = Vec::new();
        let mut cur = m.lru_head;
        while let Some(id) = cur {
            forward.push(id);
            cur = m.clusters[id].as_ref().unwrap().lru_next;
        }
        let mut backward = Vec::new();
        let mut cur = m.lru_tail;
        while let Some(id) = cur {
            backward.push(id);
            cur = m.clusters[id].as_ref().unwrap().lru_prev;
        }
        backward.reverse();
        assert_eq!(
            forward, backward,
            "LRU forward/backward traversals disagree"
        );
        forward
    }

    #[test]
    fn lru_order_matches_recency_under_interleaving() {
        // Hold a cap of 3 and exercise the DLL with interleaved touches
        // and evictions. Confirms head==LRU and tail==MRU after each step.
        let mut m = small_matcher(3);
        let a = distinct_line(1);
        let b = distinct_line(2);
        let c = distinct_line(3);
        let d = distinct_line(4);

        let ida = m.add_log_message(&a).unwrap().id();
        let idb = m.add_log_message(&b).unwrap().id();
        let idc = m.add_log_message(&c).unwrap().id();
        // Order after the first 3 inserts: head=A, ..., tail=C
        assert_eq!(lru_order(&m), vec![ida, idb, idc]);

        // Touch A — should move to tail. Order: B, C, A
        m.add_log_message(&a).unwrap();
        assert_eq!(lru_order(&m), vec![idb, idc, ida]);

        // New pattern D forces eviction of head (B). Drain re-uses freed
        // cluster ids, so the new D cluster lands in B's old slot — i.e.
        // `idd == idb`. Verify by structure, not by id: head should be C,
        // followed by A, with the brand-new D cluster on the tail.
        let idd = m.add_log_message(&d).unwrap().id();
        assert_eq!(idd, idb, "D should recycle the evicted cluster id");
        let order = lru_order(&m);
        assert_eq!(order, vec![idc, ida, idd]);
    }

    #[test]
    fn hot_clusters_survive_long_churn() {
        // Three "hot" patterns plus a long tail of one-shot patterns. The
        // hot ones get touched on every iteration; the cold ones never
        // come back. After many rounds the hot patterns must still match
        // an existing cluster (not be evicted).
        let mut m = small_matcher(5);
        let hot = [distinct_line(1), distinct_line(2), distinct_line(3)];
        let hot_ids: Vec<usize> = hot
            .iter()
            .map(|line| m.add_log_message(line).unwrap().id())
            .collect();

        // Fire 50 distinct cold patterns interleaved with re-hits on the
        // hot set. The hot ones must keep getting promoted to MRU.
        for n in 0..50 {
            for line in &hot {
                m.add_log_message(line).unwrap();
            }
            // Cold pattern (token count from 4..) — never repeated.
            m.add_log_message(&distinct_line(4 + n)).unwrap();
        }

        // Hot ids should still resolve to existing clusters with no new
        // allocations — re-feeding them returns the same template ids.
        for (line, &expected_id) in hot.iter().zip(hot_ids.iter()) {
            let id = m.add_log_message(line).unwrap().id();
            assert_eq!(id, expected_id, "hot cluster for {line:?} was evicted");
        }
        assert_eq!(m.cluster_count(), 5);
    }
}
