import assert from "node:assert/strict";
import { createServer as createHttpServer } from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { once } from "node:events";
import { test } from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import {
  StdioClientTransport,
  getDefaultEnvironment,
} from "@modelcontextprotocol/sdk/client/stdio.js";
import { EmptyResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { configuration } from "../src/server.mjs";

const entry = fileURLToPath(new URL("../src/server.mjs", import.meta.url));
const id = "ab".repeat(32);
const anchor = { height: 12, block_id: "cd".repeat(32) };

async function fixture(t, handler, settings = {}) {
  const requests = [];
  const http = createHttpServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += chunk;
    requests.push({
      method: request.method,
      path: request.url,
      body,
      headers: request.headers,
    });
    handler(request, response, body);
  });
  await new Promise((resolve) => http.listen(0, "127.0.0.1", resolve));
  const baseUrl = `http://127.0.0.1:${http.address().port}/v1`;
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [entry],
    env: { ...getDefaultEnvironment(), KADIA_API_URL: baseUrl, ...settings },
    stderr: "pipe",
  });
  let stderr = "";
  transport.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const client = new Client({ name: "kadia-adapter-test", version: "1.0.0" });
  t.after(async () => {
    await client.close();
    http.closeAllConnections();
    await new Promise((resolve) => http.close(resolve));
    assert.equal(
      stderr,
      "",
      "Successful protocol use must not log queries or responses.",
    );
  });
  await client.connect(transport);
  return { client, requests };
}

function json(response, data, status = 200, headers = {}) {
  response.writeHead(status, {
    "content-type": "application/json",
    ...headers,
  });
  response.end(JSON.stringify(data));
}

function payload(result) {
  assert.equal(result.content[0].type, "text");
  const data = JSON.parse(result.content[0].text);
  assert.deepEqual(result.structuredContent, data);
  return data;
}

test("real stdio discovery exposes only twelve read-only, bounded tools and preserves exact data/coverage", async (t) => {
  const data = {
    amount: "9223372036854775807",
    token_name: "Untrusted metadata text",
    anchor,
  };
  const run = await fixture(t, (_req, res) =>
    json(res, data, 200, { "x-explorer-completeness": "incomplete" }),
  );
  const listed = await run.client.listTools();
  assert.equal(listed.tools.length, 12);
  for (const tool of listed.tools) {
    assert.equal(tool.annotations.readOnlyHint, true);
    assert.equal(tool.annotations.destructiveHint, false);
    assert.equal(tool.annotations.idempotentHint, true);
    assert.equal(tool.inputSchema.additionalProperties, false);
  }
  assert.equal(
    listed.tools.find((tool) => tool.name === "kadia_group_balances")
      .inputSchema.properties.addresses.maxItems,
    20,
  );
  const observed = payload(
    await run.client.callTool({ name: "kadia_status", arguments: {} }),
  );
  assert.deepEqual(observed, {
    ok: true,
    data,
    completeness: "incomplete",
    status: 200,
  });
  assert.equal(run.requests[0].method, "GET");
  assert.equal(run.requests[0].path, "/v1/status");
  assert.equal(run.requests[0].headers.authorization, undefined);
  assert.equal(run.requests[0].headers.cookie, undefined);
  for (const [name, path, args] of [
    ["kadia_mempool", "/v1/mempool", {}],
    ["kadia_mining", "/v1/mining?from_height=1&to_height=12&top=3", { from_height: 1, to_height: 12, top: 3 }],
    ["kadia_address_rent_exposure", "/v1/addresses/9abc/rent?view=exposure", { addr: "9abc", view: "exposure" }],
    ["kadia_transaction", `/v1/txs/${id}`, { id }],
    ["kadia_transaction_status", `/v1/txs/${id}/status`, { id }],
    ["kadia_box", `/v1/boxes/${id}`, { id }],
    ["kadia_token", `/v1/tokens/${id}`, { id }],
    ["kadia_address", "/v1/addresses/9abc", { addr: "9abc" }],
  ]) {
    assert.equal(
      payload(await run.client.callTool({ name, arguments: args })).ok,
      true,
    );
    assert.equal(run.requests.at(-1).path, path);
  }
});

test("invalid tools, methods, oversized arguments and unknown options never reach HTTP", async (t) => {
  const run = await fixture(t, (_req, res) => json(res, {}));
  for (const [name, args] of [
    ["broadcast_transaction", { id }],
    ["kadia_box", { id: "bad" }],
    ["kadia_status", { url: "https://example.invalid" }],
    ["kadia_status", { method: "DELETE" }],
    ["kadia_mining", { from_height: 2, to_height: 1 }],
    ["kadia_mining", { from_height: 1, to_height: 20161 }],
    ["kadia_mining", { from_height: 1, to_height: 12, top: 51 }],
    ["kadia_address_rent_exposure", { addr: "9abc" }],
    ["kadia_address_rent_exposure", { addr: "9abc", view: "legacy" }],
    ["kadia_group_balances", { addresses: Array(21).fill("9abc") }],
    ["kadia_address", { addr: "a".repeat(4097) }],
    ["kadia_address", { addr: "é".repeat(3000) }],
    ["kadia_address_activity", { addr: "9abc", limit: 101 }],
    ["kadia_address_activity", { addr: "9abc", cursor: "x" }],
    ["kadia_address_activity", { addr: "9abc", cursor: "", snapshot: "" }],
    ["kadia_address_activity", { addr: "9abc", from_ms: 12, to_ms: 11 }],
    ["kadia_address_activity", { addr: "9abc", from_ms: 12, to_ms: 12 }],
    [
      "kadia_compare_balances",
      { addr: "9abc", from_height: 12, to_height: 11 },
    ],
    [
      "kadia_compare_balances",
      { addr: "9abc", from_height: 0, to_height: 12, from_block_id: id },
    ],
  ]) {
    const result = await run.client.callTool({ name, arguments: args });
    assert.equal(
      result.isError,
      true,
      name + JSON.stringify(args).slice(0, 100),
    );
  }
  await assert.rejects(
    run.client.request(
      { method: "transactions/broadcast", params: {} },
      EmptyResultSchema,
    ),
    /not found/i,
  );
  assert.equal(run.requests.length, 0);
});

test("group selection uses only the read-only POST and comparison keeps both pins", async (t) => {
  const run = await fixture(t, (_req, res) =>
    json(res, { anchor, value: "9007199254740993" }),
  );
  const selected = ["9abc", "9def"];
  const result = payload(
    await run.client.callTool({
      name: "kadia_group_balances",
      arguments: { addresses: selected },
    }),
  );
  assert.equal(result.completeness, "unknown");
  assert.deepEqual(run.requests[0], {
    ...run.requests[0],
    method: "POST",
    path: "/v1/addresses/balances",
    body: JSON.stringify({ addresses: selected }),
  });
  await run.client.callTool({
    name: "kadia_compare_balances",
    arguments: {
      addr: "9abc",
      from_height: 10,
      to_height: 12,
      from_block_id: id,
      to_block_id: anchor.block_id,
    },
  });
  const query = new URL("http://local" + run.requests[1].path);
  assert.equal(query.pathname, "/v1/addresses/9abc/balance/compare");
  assert.equal(query.searchParams.get("from_block_id"), id);
  assert.equal(query.searchParams.get("to_block_id"), anchor.block_id);
  assert.equal(run.requests[1].method, "GET");
});

test("empty filtered activity pages preserve continuation and never fetch another page automatically", async (t) => {
  const page = {
    consistency: "strict",
    anchor,
    observed_anchor: anchor,
    items: [],
    next_cursor: "c1",
    next_snapshot: "s1",
    scan_complete: false,
  };
  const run = await fixture(t, (_req, res) => json(res, page));
  assert.deepEqual(
    payload(
      await run.client.callTool({
        name: "kadia_address_activity",
        arguments: { addr: "9abc", direction: "received" },
      }),
    ).data,
    page,
  );
  assert.equal(run.requests.length, 1);
  const first = new URL("http://local" + run.requests[0].path);
  assert.equal(first.searchParams.get("consistency"), "strict");
  assert.equal(first.searchParams.get("limit"), "25");
  await run.client.callTool({
    name: "kadia_address_activity",
    arguments: {
      addr: "9abc",
      direction: "received",
      cursor: "c1",
      snapshot: "s1",
      limit: 10,
    },
  });
  assert.equal(run.requests.length, 2);
  assert.ok(run.requests[1].path.includes("cursor=c1&snapshot=s1"));
});

test("API reorg, preparing, admission and rate-limit errors retain exact status/problem/retry metadata", async (t) => {
  let status = 409;
  const run = await fixture(t, (_req, res) =>
    json(
      res,
      {
        code: "fixture_error",
        detail: "Explicit API error",
        raw: "9223372036854775807",
      },
      status,
      { "retry-after": "3" },
    ),
  );
  for (status of [409, 503, 422, 429]) {
    const result = await run.client.callTool({
      name: "kadia_status",
      arguments: {},
    });
    assert.equal(result.isError, true);
    assert.deepEqual(payload(result), {
      ok: false,
      error: {
        kind: "api",
        status,
        code: "fixture_error",
        retry_after: "3",
        problem: {
          code: "fixture_error",
          detail: "Explicit API error",
          raw: "9223372036854775807",
        },
      },
    });
  }
  assert.equal(run.requests.length, 4, "No automatic retries");
});

test("declared and streamed response limits return tool errors instead of truncated facts", async (t) => {
  let declared = true;
  const run = await fixture(
    t,
    (_req, res) => {
      res.writeHead(200, {
        "content-type": "application/json",
        ...(declared ? { "content-length": "3000" } : {}),
      });
      res.write('{"large":"');
      res.end("x".repeat(2000) + '"}');
    },
    { KADIA_MAX_RESPONSE_BYTES: "1024" },
  );
  for (declared of [true, false]) {
    const result = await run.client.callTool({
      name: "kadia_status",
      arguments: {},
    });
    assert.equal(result.isError, true);
    assert.equal(payload(result).error.kind, "limit");
    assert.equal(payload(result).data, undefined);
  }
});

test("timeouts and redirect refusal never become empty successful observations", async (t) => {
  let mode = "timeout";
  const run = await fixture(
    t,
    (_req, res) => {
      if (mode === "timeout") {
        setTimeout(() => res.end("{}"), 250).unref();
        return;
      }
      res.writeHead(302, { location: "http://127.0.0.1:1/do-not-follow" });
      res.end();
    },
    { KADIA_TIMEOUT_MS: "100" },
  );
  let result = await run.client.callTool({
    name: "kadia_status",
    arguments: {},
  });
  assert.equal(result.isError, true);
  assert.equal(payload(result).error.kind, "timeout");
  mode = "redirect";
  result = await run.client.callTool({ name: "kadia_status", arguments: {} });
  assert.equal(payload(result).error.kind, "request_failed");
  assert.equal(run.requests.length, 2);
});

test("four concurrent observations are admitted and excess work is not queued", async (t) => {
  const pending = [];
  const run = await fixture(t, (_req, res) => pending.push(res));
  const calls = Array.from({ length: 4 }, () =>
    run.client.callTool({ name: "kadia_status", arguments: {} }),
  );
  for (let i = 0; i < 100 && pending.length < 4; i++)
    await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(pending.length, 4);
  const excess = await run.client.callTool({
    name: "kadia_status",
    arguments: {},
  });
  assert.equal(payload(excess).error.kind, "busy");
  assert.equal(run.requests.length, 4);
  pending.forEach((res) => json(res, { tip_height: 12 }));
  assert.ok((await Promise.all(calls)).every((result) => payload(result).ok));
});

test("configuration is explicit and invalid startup never prints a protocol-breaking banner", async () => {
  assert.throws(() => configuration({}), /KADIA_API_URL is required/);
  assert.throws(
    () =>
      configuration({
        KADIA_API_URL: "http://localhost/v1",
        KADIA_TIMEOUT_MS: "0",
      }),
    /integer/,
  );
  assert.throws(
    () =>
      configuration({
        KADIA_API_URL: "http://localhost/v1",
        KADIA_MAX_RESPONSE_BYTES: "2097153",
      }),
    /between/,
  );
  for (const url of [
    "",
    "file:///private",
    "https://user:secret@example.test/v1",
    "https://example.test/v1?secret=private",
  ]) {
    const child = spawn(process.execPath, [entry], {
      env: { ...getDefaultEnvironment(), KADIA_API_URL: url },
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "",
      stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    const [code] = await once(child, "close");
    assert.equal(code, 1);
    assert.equal(stdout, "");
    assert.match(stderr, /startup failed/);
    assert.doesNotMatch(stderr, /secret|private|example\.test/);
  }
});

test("protocol cancellation aborts an admitted observation without retrying", async (t) => {
  let hanging = true;
  const run = await fixture(t, (_req, res) => {
    if (!hanging) json(res, { height: 12 });
  });
  const cancellation = new AbortController();
  const request = run.client.callTool(
    { name: "kadia_status", arguments: {} },
    undefined,
    { signal: cancellation.signal },
  );
  for (let i = 0; i < 100 && run.requests.length === 0; i++)
    await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(run.requests.length, 1);
  cancellation.abort(new Error("Test cancellation"));
  await assert.rejects(request, /cancellation/i);
  hanging = false;
  assert.equal(
    payload(await run.client.callTool({ name: "kadia_status", arguments: {} }))
      .ok,
    true,
  );
  assert.equal(run.requests.length, 2);
});

test("the stdio transport closes an oversized message before JSON parsing", async (t) => {
  const child = spawn(process.execPath, [entry], {
    env: { ...getDefaultEnvironment(), KADIA_API_URL: "http://127.0.0.1:1/v1" },
    windowsHide: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stdout = "",
    stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  child.stdin.on("error", () => {});
  const timeout = setTimeout(() => child.kill(), 5000);
  t.after(() => {
    clearTimeout(timeout);
    child.kill();
  });
  child.stdin.write("x".repeat(131073));
  const [code] = await once(child, "close");
  assert.equal(code, 1);
  assert.equal(stdout, "");
  assert.match(stderr, /protocol\/transport error/);
});
