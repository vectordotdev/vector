import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

// Deliberately preserve untagged manifests: they can be children of a retained
// multi-platform index, including its provenance attestations.
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
  return {
    api,
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

export async function cleanup({ registry, today, apply = false, log = console.log }) {
  const cutoff = cutoffDate(today, 14);
  const before = await registry.versions();
  const candidates = before.filter((version) => eligible(version, cutoff));
  const result = { cutoff, apply, scanned: before.length, candidates, deleted: [], skipped: [] };
  log(
    `${apply ? "APPLY" : "DRY RUN"}: ${candidates.length} candidate(s) out of ${before.length} versions; on or before ${cutoff}`
  );
  for (const version of candidates) {
    log(`${apply ? "DELETE" : "WOULD DELETE"} ${version.id}: ${version.metadata.container.tags.join(", ")}`);
    if (!apply) continue;
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
