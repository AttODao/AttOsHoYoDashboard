// Tests for publish-release.sh against a local mock of the Forgejo release API.
// Run: node scripts/publish-release.test.mjs (needs bash, curl, jq and coreutils on PATH).
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";

const script = new URL("./publish-release.sh", import.meta.url).pathname;
const prefix = "/api/v1/repos/owner/repo";

function mockForgejo({ existing = null, corrupt = null } = {}) {
  const state = { release: existing, assets: existing ? [...existing.assets] : [], calls: [], nextAsset: 100 };
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://mock");
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks);
    state.calls.push(`${req.method} ${url.pathname}`);
    const json = (status, value) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(value));
    };
    const release = () => ({
      ...state.release,
      assets: state.assets.map(({ id, name, data }) => ({
        id,
        name,
        size: data.length,
        browser_download_url: `http://127.0.0.1:${server.address().port}/download/${encodeURIComponent(name)}`,
      })),
    });
    if (req.headers.authorization !== "token synthetic-token" && !url.pathname.startsWith("/download/")) {
      return json(401, { message: "token" });
    }
    if (req.method === "GET" && url.pathname === `${prefix}/releases/tags/v1.8.0-test.1`) {
      return state.release ? json(200, release()) : json(404, { message: "not found" });
    }
    if (req.method === "POST" && url.pathname === `${prefix}/releases`) {
      const request = JSON.parse(body);
      state.created = request;
      state.release = { id: 7, tag_name: request.tag_name, draft: request.draft, prerelease: request.prerelease };
      return json(201, release());
    }
    if (req.method === "GET" && url.pathname === `${prefix}/releases/7/assets`) {
      return json(200, release().assets);
    }
    const asset = url.pathname.match(new RegExp(`^${prefix}/releases/7/assets/(\\d+)$`));
    if (req.method === "DELETE" && asset) {
      state.assets = state.assets.filter(({ id }) => id !== Number(asset[1]));
      res.writeHead(204).end();
      return;
    }
    if (req.method === "POST" && url.pathname === `${prefix}/releases/7/assets`) {
      const form = await new Request("http://mock", { method: "POST", headers: req.headers, body }).formData();
      const data = Buffer.from(await form.get("attachment").arrayBuffer());
      state.assets.push({ id: state.nextAsset++, name: url.searchParams.get("name"), data });
      return json(201, {});
    }
    const download = url.pathname.match(/^\/download\/(.+)$/);
    if (req.method === "GET" && download) {
      const name = decodeURIComponent(download[1]);
      const found = state.assets.find((entry) => entry.name === name);
      if (!found) return json(404, {});
      const data = Buffer.from(found.data);
      if (name === corrupt) data[0] ^= 0xff;
      res.writeHead(200).end(data);
      return;
    }
    json(500, { message: `unexpected ${req.method} ${url.pathname}` });
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve({ server, state })));
}

function dist(files, sums = files) {
  const dir = mkdtempSync(join(tmpdir(), "publish-release-"));
  const lines = [];
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), content);
  }
  for (const [name, content] of Object.entries(sums)) {
    lines.push(`${createHash("sha256").update(content).digest("hex")}  ${name}`);
  }
  writeFileSync(join(dir, "SHA256SUMS"), `${lines.sort().join("\n")}\n`);
  return dir;
}

function publish(port, dir, prerelease = "true") {
  return new Promise((resolve) => {
    const child = spawn("bash", [script, dir], {
      env: {
        ...process.env,
        API: `http://127.0.0.1:${port}${prefix}`,
        FORGEJO_TOKEN: "synthetic-token",
        TAG: "v1.8.0-test.1",
        PRERELEASE: prerelease,
        RELEASE_BODY: "synthetic body",
      },
    });
    let output = "";
    child.stdout.on("data", (data) => (output += data));
    child.stderr.on("data", (data) => (output += data));
    child.on("close", (code) => resolve({ code, output }));
  });
}

async function scenario(name, options, files, check, sums) {
  const { server, state } = await mockForgejo(options);
  const dir = dist(files, sums);
  try {
    const result = await publish(server.address().port, dir, options.prerelease);
    await check(result, state);
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    throw error;
  } finally {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

const apk = { "ahd-android-1.8.0-test.1-arm64.apk": "synthetic apk" };

await scenario("creates a missing prerelease and verifies every asset", {}, apk, ({ code, output }, state) => {
  assert.equal(code, 0, output);
  assert.deepEqual(state.created, {
    tag_name: "v1.8.0-test.1",
    name: "v1.8.0-test.1",
    body: "synthetic body",
    draft: false,
    prerelease: true,
  });
  assert.deepEqual(state.assets.map(({ name }) => name).sort(), ["SHA256SUMS", "ahd-android-1.8.0-test.1-arm64.apk"]);
  assert.match(output, /2 assets published and verified/);
});

await scenario(
  "replaces same-named assets of an existing release and keeps others",
  {
    existing: {
      id: 7,
      tag_name: "v1.8.0-test.1",
      draft: false,
      prerelease: true,
      assets: [
        { id: 1, name: "ahd-android-1.8.0-test.1-arm64.apk", data: Buffer.from("stale") },
        { id: 2, name: "notes.txt", data: Buffer.from("kept") },
      ],
    },
  },
  apk,
  ({ code, output }, state) => {
    assert.equal(code, 0, output);
    assert.equal(state.created, undefined);
    assert.ok(state.calls.includes(`DELETE ${prefix}/releases/7/assets/1`));
    const names = state.assets.map(({ name }) => name).sort();
    assert.deepEqual(names, ["SHA256SUMS", "ahd-android-1.8.0-test.1-arm64.apk", "notes.txt"]);
    const uploaded = state.assets.find(({ name }) => name.endsWith(".apk"));
    assert.equal(uploaded.data.toString(), "synthetic apk");
  },
);

await scenario(
  "fails when a downloaded asset does not match SHA256SUMS",
  { corrupt: "ahd-android-1.8.0-test.1-arm64.apk" },
  apk,
  ({ code, output }) => {
    assert.notEqual(code, 0, output);
    assert.match(output, /FAILED/);
  },
);

await scenario(
  "fails when an existing release has another prerelease flag",
  { existing: { id: 7, tag_name: "v1.8.0-test.1", draft: false, prerelease: true, assets: [] }, prerelease: "false" },
  apk,
  ({ code, output }) => {
    assert.notEqual(code, 0, output);
    assert.match(output, /prerelease flag is not false/);
  },
);

await scenario(
  "refuses to publish files that SHA256SUMS does not list",
  {},
  { ...apk, "extra.bin": "unchecked" },
  ({ code, output }, state) => {
    assert.notEqual(code, 0, output);
    assert.match(output, /SHA256SUMS lists/);
    assert.deepEqual(state.calls, []);
  },
  apk,
);
