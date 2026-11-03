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
  const state = new Map([
    [1, version(1, ["nightly-2026-09-01-alpine"])],
    [2, version(2, ["nightly-2026-09-01-debian"])]
  ]);
  const registry = {
    versions: async () => [...state.values()],
    manifest: async () => ({}),
    api: async (path, method = "GET") => {
      calls.push([path, method]);
      const id = Number(path.split("/").at(-1));
      if (method === "DELETE") state.delete(id);
      else {
        if (id === 2) state.set(id, version(id, ["nightly-debian"]));
        return state.get(id);
      }
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

function mockRegistry(initial, manifests) {
  const state = new Map(initial.map((v) => [v.id, v]));
  const calls = [];
  const registry = {
    versions: async () => [...state.values()],
    manifest: async (digest) => manifests[digest] ?? {},
    api: async (path, method = "GET") => {
      const id = Number(path.split("/").at(-1));
      calls.push([id, method]);
      if (method === "DELETE") state.delete(id);
      else return state.get(id);
    }
  };
  return { state, calls, registry };
}

test("nested indexes, shared expired children, and OCI subject attestations are removed once", async () => {
  const { state, registry } = mockRegistry(
    [
      version(1, ["nightly-2026-09-01-alpine"]),
      version(2, ["nightly-2026-09-01-debian"]),
      version(3, []),
      version(4, []),
      version(5, [])
    ],
    {
      "sha256:1": { manifests: [{ digest: "sha256:3" }] },
      "sha256:2": { manifests: [{ digest: "sha256:3" }] },
      "sha256:3": { manifests: [{ digest: "sha256:4" }] },
      "sha256:5": { subject: { digest: "sha256:4" } }
    }
  );
  const options = { registry, today: "2026-11-03", log: () => {} };
  const dryRun = await cleanup(options);
  assert.equal(state.size, 5);
  assert.deepEqual(dryRun.candidates.map((v) => v.id).sort(), [1, 2, 3, 4, 5]);
  const result = await cleanup({ ...options, apply: true });
  assert.deepEqual([...result.deleted].sort(), [1, 2, 3, 4, 5]);
  assert.equal(state.size, 0);
});

test("even an expired tagged manifest is protected if a retained index references it", async () => {
  const { state, registry } = mockRegistry(
    [version(1, ["nightly-2026-09-01-alpine"]), version(2, ["0.62.0-alpine"]), version(3, [])],
    {
      "sha256:2": { manifests: [{ digest: "sha256:1" }] },
      "sha256:1": { manifests: [{ digest: "sha256:3" }] }
    }
  );
  const result = await cleanup({ registry, today: "2026-11-03", apply: true, log: () => {} });
  assert.deepEqual(result.deleted, []);
  assert.equal(state.size, 3);
});

test("a manifest read failure aborts before any deletion", async () => {
  const { registry, calls } = mockRegistry(
    [version(1, ["nightly-2026-09-01-alpine"]), version(2, ["0.62.0-alpine"])],
    {}
  );
  registry.manifest = async () => {
    throw new Error("HTTP 503");
  };
  await assert.rejects(cleanup({ registry, today: "2026-11-03", apply: true }), /HTTP 503/);
  assert.deepEqual(calls, []);
});

test("a new retained index published during root deletion protects its shared child", async () => {
  const { registry, state } = mockRegistry([version(1, ["nightly-2026-09-01-alpine"]), version(3, [])], {
    "sha256:1": { manifests: [{ digest: "sha256:3" }] },
    "sha256:2": { manifests: [{ digest: "sha256:3" }] }
  });
  const original = registry.api;
  registry.api = async (path, method) => {
    const result = await original(path, method);
    if (method === "DELETE") state.set(2, version(2, ["nightly-2026-11-03-alpine"]));
    return result;
  };
  const result = await cleanup({ registry, today: "2026-11-03", apply: true, log: () => {} });
  assert.deepEqual(result.deleted, [1]);
  assert.deepEqual([...state.keys()].sort(), [2, 3]);
});

test("registry manifest authentication is scoped and exchanged once for parallel reads", async () => {
  let tokenRequests = 0;
  const registry = client({
    owner: "vectordotdev",
    packageName: "ci-sandbox/mock",
    token: "mock-token",
    request: async (url) => {
      if (url.includes("/token?")) {
        tokenRequests++;
        assert(url.includes("scope=repository%3Avectordotdev%2Fci-sandbox%2Fmock%3Apull"));
        return { ok: true, json: async () => ({ token: "mock-registry-token" }) };
      }
      assert(url.startsWith("https://ghcr.io/v2/vectordotdev/ci-sandbox/mock/manifests/"));
      return { ok: true, json: async () => ({ schemaVersion: 2 }) };
    }
  });
  await Promise.all([registry.manifest("sha256:1"), registry.manifest("sha256:2")]);
  assert.equal(tokenRequests, 1);
});

test("expired image cleanup removes exclusive children and preserves children shared with retained images", async () => {
  const initial = [
    version(1, ["nightly-2026-09-01-alpine"]),
    version(2, ["nightly-2026-11-01-alpine"]),
    version(3, []), // child shared by both indexes
    version(4, []), // expired image's exclusive architecture manifest
    version(5, []), // expired image's exclusive attestation
    version(6, []) // unrelated orphan
  ];
  const state = new Map(initial.map((v) => [v.id, v]));
  const manifests = new Map([
    ["sha256:1", { manifests: [{ digest: "sha256:3" }, { digest: "sha256:4" }, { digest: "sha256:5" }] }],
    ["sha256:2", { manifests: [{ digest: "sha256:3" }] }]
  ]);
  const registry = {
    versions: async () => [...state.values()],
    manifest: async (digest) => manifests.get(digest) ?? {},
    api: async (path, method = "GET") => {
      const id = Number(path.split("/").at(-1));
      if (method === "DELETE") state.delete(id);
      else return state.get(id);
    }
  };
  const result = await cleanup({ registry, today: "2026-11-03", apply: true, log: () => {} });
  assert.deepEqual([...result.deleted].sort(), [1, 4, 5]);
  assert.deepEqual([...state.keys()].sort(), [2, 3, 6]);
});
