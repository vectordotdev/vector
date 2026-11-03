import assert from "node:assert/strict";
import test from "node:test";
import { cleanup, client, cutoffDate, eligible } from "./cleanup.mjs";

const version = (id, tags) => ({ id, name: `sha256:${id}`, metadata: { container: { tags } } });
const cutoff = cutoffDate("2026-11-03", 14);

test("retention boundary uses the UTC tag date, not package creation time", () => {
  assert.equal(cutoff, "2026-10-20");
  assert(eligible({ ...version(1, ["nightly-2026-10-19-alpine"]), created_at: "2026-11-03T00:00:00Z" }, cutoff));
  assert(eligible(version(2, ["nightly-2026-10-20-alpine"]), cutoff));
  assert(!eligible(version(3, ["nightly-2026-10-21-alpine"]), cutoff));
});

test("every tag must be a valid expired nightly for a known variant", () => {
  for (const base of ["alpine", "debian", "distroless-static", "distroless-libc"]) {
    assert(eligible(version(1, [`nightly-2026-09-01-${base}`]), cutoff));
  }
  for (const tags of [
    [],
    ["nightly-alpine"],
    ["latest-alpine"],
    ["0.62.0-alpine"],
    ["nightly-2026-09-01-alpine", "nightly-alpine"],
    ["nightly-2026-09-01-alpine", "0.62.0-alpine"],
    ["nightly-2026-09-01-alpine", "nightly-2026-11-01-alpine"],
    ["nightly-2026-02-30-alpine"],
    ["nightly-2026-09-01-ubuntu"]
  ])
    assert(!eligible(version(1, tags), cutoff), JSON.stringify(tags));
  assert(!eligible({ id: 1 }, cutoff));
});

test("invalid retention inputs fail closed", () => {
  for (const days of [0, -1, 0.5, NaN]) assert.throws(() => cutoffDate("2026-11-03", days));
  assert.throws(() => cutoffDate("2026-02-30", 14));
});

test("package path is encoded and every page is read", async () => {
  const urls = [];
  const registry = client({
    owner: "vectordotdev",
    packageName: "ci-sandbox/mock",
    token: "mock-token",
    request: async (url) => {
      urls.push(url);
      return {
        ok: true,
        status: 200,
        json: async () =>
          urls.length === 1 ? Array.from({ length: 100 }, (_, id) => version(id, [])) : [version(100, [])]
      };
    }
  });
  assert.equal((await registry.versions()).length, 101);
  assert(urls[0].includes("/ci-sandbox%2Fmock/versions?per_page=100&page=1"));
  assert(urls[1].endsWith("page=2"));
});

test("HTTP errors fail instead of treating an incomplete listing as success", async () => {
  const registry = client({
    owner: "o",
    packageName: "p",
    token: "mock",
    request: async () => ({ ok: false, status: 403 })
  });
  await assert.rejects(registry.versions(), /HTTP 403/);
});

test("dry run performs no deletes; apply rechecks protected tags", async () => {
  const calls = [];
  const registry = {
    versions: async () => [version(1, ["nightly-2026-09-01-alpine"]), version(2, ["nightly-2026-09-01-debian"])],
    api: async (path, method = "GET") => {
      calls.push([path, method]);
      return version(
        path.endsWith("/1") ? 1 : 2,
        path.endsWith("/1") ? ["nightly-2026-09-01-alpine"] : ["nightly-debian"]
      );
    }
  };
  const options = { registry, today: "2026-11-03", log: () => {} };
  assert.equal((await cleanup(options)).candidates.length, 2);
  assert.deepEqual(calls, []);
  const result = await cleanup({ ...options, apply: true });
  assert.deepEqual(result.deleted, [1]);
  assert.deepEqual(result.skipped, [2]);
  assert.deepEqual(calls, [
    ["/versions/1", "GET"],
    ["/versions/1", "DELETE"],
    ["/versions/2", "GET"]
  ]);
});
