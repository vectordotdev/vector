import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

// Tags select expired roots; manifest references determine which children can
// also be deleted without affecting another image.
const datedNightly = /^nightly-(\d{4}-\d{2}-\d{2})-(alpine|debian|distroless-static|distroless-libc)$/;

export function cutoffDate(today, retentionDays) {
  assert(Number.isInteger(retentionDays) && retentionDays > 0, "retentionDays must be a positive integer");
  const date = new Date(`${today}T00:00:00Z`);
  assert(!Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === today, "Invalid UTC date");
  date.setUTCDate(date.getUTCDate() - retentionDays);
  return date.toISOString().slice(0, 10);
}

export function eligible(version, cutoff) {
  const tags = version.metadata?.container?.tags;
  return (
    Array.isArray(tags) &&
    tags.length > 0 &&
    tags.every((tag) => {
      const match = datedNightly.exec(tag);
      if (!match) return false;
      const date = new Date(`${match[1]}T00:00:00Z`);
      return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === match[1] && match[1] <= cutoff;
    })
  );
}

export function client({ owner, packageName, token, request = fetch }) {
  assert(owner && packageName && token, "Owner, package name and token are required");
  const path = `/orgs/${encodeURIComponent(owner)}/packages/container/${encodeURIComponent(packageName)}`;
  async function api(suffix = "", method = "GET") {
    const response = await request(`https://api.github.com${path}${suffix}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    });
    if (!response.ok) throw new Error(`${method} ${path}${suffix}: HTTP ${response.status}`);
    return response.status === 204 ? undefined : response.json();
  }
  let pullToken;
  const manifestCache = new Map();
  async function authenticate() {
    const scope = encodeURIComponent(`repository:${owner}/${packageName}:pull`);
    const response = await request(`https://ghcr.io/token?service=ghcr.io&scope=${scope}`, {
      headers: { Authorization: `Basic ${Buffer.from(`token:${token}`).toString("base64")}` }
    });
    if (!response.ok) throw new Error(`GHCR authentication: HTTP ${response.status}`);
    const credentials = await response.json();
    assert(credentials.token, "GHCR did not return a pull token");
    return credentials.token;
  }
  return {
    api,
    async manifest(digest) {
      if (!manifestCache.has(digest)) {
        // The promise prevents parallel manifest requests from exchanging the
        // same credential repeatedly. Credentials stay in memory.
        pullToken ??= authenticate();
        const response = await request(`https://ghcr.io/v2/${owner}/${packageName}/manifests/${digest}`, {
          headers: {
            Authorization: `Bearer ${await pullToken}`,
            Accept:
              "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json"
          }
        });
        if (!response.ok) throw new Error(`GET manifest ${digest}: HTTP ${response.status}`);
        const manifest = await response.json();
        assert.equal(manifest.schemaVersion, 2, `Unsupported manifest ${digest}`);
        manifestCache.set(digest, manifest);
      }
      return manifestCache.get(digest);
    },
    async versions() {
      const versions = [];
      for (let page = 1; ; page++) {
        const batch = await api(`/versions?per_page=100&page=${page}`);
        assert(Array.isArray(batch), "Expected an array of versions");
        versions.push(...batch);
        if (batch.length < 100) return versions;
      }
    }
  };
}

async function graphFor(registry, versions) {
  const graph = new Map(versions.map((version) => [version.name, new Set()]));
  const queue = [...versions];
  // Read every manifest before deleting anything. A failed read aborts cleanup;
  // otherwise an unseen parent could cause a shared child to be deleted.
  await Promise.all(
    Array.from({ length: Math.min(8, queue.length) }, async () => {
      while (queue.length) {
        const version = queue.shift();
        const manifest = await registry.manifest(version.name);
        for (const child of manifest.manifests ?? []) {
          assert(child.digest, `Missing child digest in ${version.name}`);
          graph.get(version.name).add(child.digest);
        }
        // OCI 1.1 referrers belong to their subject even when the index does not
        // list them. Treat these as another child of the subject.
        if (manifest.subject?.digest) {
          const subject = manifest.subject.digest;
          if (!graph.has(subject)) graph.set(subject, new Set());
          graph.get(subject).add(version.name);
        }
      }
    })
  );
  return graph;
}

function reachable(roots, graph) {
  const visited = new Set();
  const queue = [...roots];
  while (queue.length) {
    const digest = queue.pop();
    if (visited.has(digest)) continue;
    visited.add(digest);
    queue.push(...(graph.get(digest) ?? []));
  }
  return visited;
}

export async function cleanup({ registry, today, apply = false, log = console.log }) {
  const cutoff = cutoffDate(today, 14);
  const before = await registry.versions();
  const nightlyCandidates = before.filter((version) => eligible(version, cutoff));
  const result = {
    cutoff,
    apply,
    scanned: before.length,
    nightlyCandidates,
    candidates: [],
    shared: [],
    deleted: [],
    skipped: []
  };
  if (!nightlyCandidates.length) return result;
  const graph = await graphFor(registry, before);
  const expiredRoots = new Set(nightlyCandidates.map((version) => version.name));
  const expiredTree = reachable(expiredRoots, graph);
  // Protect tagged images being retained and everything outside the expired
  // trees. This includes unrelated untagged images, not just release tags.
  const retained = reachable(
    before
      .filter(
        (version) =>
          !expiredRoots.has(version.name) &&
          (version.metadata.container.tags.length > 0 || !expiredTree.has(version.name))
      )
      .map((version) => version.name),
    graph
  );
  result.candidates = before.filter((version) => expiredTree.has(version.name) && !retained.has(version.name));
  result.shared = before.filter((version) => expiredTree.has(version.name) && retained.has(version.name));
  log(
    `${apply ? "APPLY" : "DRY RUN"}: ${result.candidates.length} candidate(s), including exclusive children, out of ${before.length} versions; on or before ${cutoff}`
  );
  if (!apply) {
    for (const version of result.candidates) log(`WOULD DELETE ${version.id}: ${version.name}`);
    return result;
  }
  const candidateDigests = new Set(result.candidates.map((version) => version.name));
  for (const version of nightlyCandidates.filter((version) => candidateDigests.has(version.name))) {
    // Recheck tags immediately before deleting, protecting a version that gained
    // a release tag or moving alias after the original listing.
    const current = await registry.api(`/versions/${version.id}`);
    if (!eligible(current, cutoff)) {
      result.skipped.push(version.id);
      log(`SKIP ${version.id}: tags changed`);
      continue;
    }
    await registry.api(`/versions/${version.id}`, "DELETE");
    result.deleted.push(version.id);
    log(`DELETED nightly ${version.id}: ${version.name}`);
  }
  // Refresh references after deleting roots. A retained image may have acquired
  // a child while roots were being deleted, or a candidate may have gained a
  // protected tag. Preserve its full tree before removing any children.
  const remaining = await registry.versions();
  const currentGraph = await graphFor(registry, remaining);
  const protectedNow = reachable(
    remaining
      .filter((version) => version.metadata.container.tags.length > 0 || !candidateDigests.has(version.name))
      .map((version) => version.name),
    currentGraph
  );
  for (const version of remaining.filter(
    (version) => candidateDigests.has(version.name) && !protectedNow.has(version.name)
  )) {
    const current = await registry.api(`/versions/${version.id}`);
    if (current.metadata.container.tags.length > 0) {
      for (const digest of reachable([current.name], currentGraph)) protectedNow.add(digest);
      result.skipped.push(version.id);
      continue;
    }
    if (protectedNow.has(version.name)) continue;
    await registry.api(`/versions/${version.id}`, "DELETE");
    result.deleted.push(version.id);
    log(`DELETED exclusive child ${version.id}: ${version.name}`);
  }
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  assert(
    process.argv.slice(2).every((arg) => arg === "--apply"),
    "Only --apply is accepted; default is dry run"
  );
  const result = await cleanup({
    registry: client({
      owner: process.env.GHCR_OWNER,
      packageName: process.env.GHCR_PACKAGE,
      token: process.env.GH_TOKEN
    }),
    today: new Date().toISOString().slice(0, 10),
    apply: process.argv.includes("--apply")
  });
  console.log(JSON.stringify(result, null, 2));
}
