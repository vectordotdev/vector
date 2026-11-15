# Ordering-comparison counter (not a timing benchmark)

Run: `cargo +1.96.1 run --release --offline > counts.csv`.

This uses an isolated copy of Vector-pinned VRL, not the sibling VRL repository
or either benchmark image. Two files are instrumented: KeyString's derived
ordering is replaced with the same lexicographic ordering plus an atomic counter;
CRUD borrowed get/get_mut lookups convert their query to KeyString so those tree
comparisons also pass through the counter. This adds query allocations but keeps
the ordering and search decisions. Never use this build's timings for comparison.

The bulk helper is extracted from the actual Vector working tree at build time.
The baseline uses per-key Value::insert. Both results must be equal. Counters are
reset after fixture preparation and read before equality checks.

Scope: ordering comparisons while inserting pod labels and annotations into one
synthetic legacy-namespace Value event. The input case with 21 labels uses the
same sorted label/annotation names as the throughput workload: 20 bench.example
labels plus the selector label, and 10 bench.example annotations. The synthetic
event starts with message/timestamp/stream/file fields and three pod identity
fields. Existing-object cases start with one retained key in each destination.

This is an exact counter for this fixture's instrumented operations, not an
invocation count from a live Kubernetes collector. Real counts depend on event
contents, tree shape, namespace, custom paths and metadata. It does not count
individual compared bytes, input BTreeMap construction, parsing, or other metadata
fields. Release optimizations and libc affect CPU time separately from count.
