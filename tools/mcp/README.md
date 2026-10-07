# Kadia read-only MCP adapter

A local stdio server for explicitly requested Ergo explorer observations. It reuses
the checked-in portable client and operation catalog in `frontend/static/sdk/`.
Keep this directory inside the repository; it is not a standalone published package.
No HTTP service, daemon, wallet access, transaction signing or broadcast is provided.

Requires Node.js 22 or newer. Install locked local dependencies from this directory:

```sh
npm ci --ignore-scripts
npm test
```

Configure your MCP host to launch **Node directly** with an absolute server path:

```json
{
  "mcpServers": {
    "kadia": {
      "command": "node",
      "args": ["/absolute/path/to/kadia-explorer/tools/mcp/src/server.mjs"],
      "env": {
        "KADIA_API_URL": "http://127.0.0.1:8080/v1"
      }
    }
  }
}
```

The example port is a placeholder: use your actual API address. Windows paths can
use forward slashes or escaped backslashes. Do not launch `npm start` through an
MCP host: npm's own banner can pollute the protocol channel. The adapter itself
prints only MCP messages to stdout; startup/transport diagnostics go to stderr.

`KADIA_API_URL` is mandatory and includes the API root, usually `/v1`. HTTP and
HTTPS are accepted; credentials, query strings and fragments are rejected.
Queries go only to this configured API, which can observe the selected addresses
and IDs. The adapter stores no selections or responses and sends no cookies or
authorization headers. Redirects are refused. It never follows URLs in token
metadata or other response fields. Chain metadata remains untrusted data.

## Tools and scope

| Tool                       | SDK operation       | Scope                                                        |
| -------------------------- | ------------------- | ------------------------------------------------------------ |
| `kadia_status`             | `status`            | Indexed tip, readiness and health                            |
| `kadia_mempool`            | `mempool`           | Up to 100 pending summaries from the configured primary node |
| `kadia_mining`             | `mining`            | Keys, versions and raw votes over at most 20,160 canonical headers |
| `kadia_address_rent_exposure` | `addressRentExposure` | Compact indexed unspent scan, with `view: "exposure"` and coverage |
| `kadia_transaction`        | `transaction`       | One expanded indexed transaction                             |
| `kadia_transaction_status` | `transactionStatus` | One inclusion/mempool observation                            |
| `kadia_box`                | `box`               | One indexed box                                              |
| `kadia_address`            | `address`           | Current indexed address record                               |
| `kadia_token`              | `token`             | One token mint/supply record                                 |
| `kadia_group_balances`     | `groupBalances`     | Up to 20 explicitly supplied addresses                       |
| `kadia_compare_balances`   | `compareBalances`   | Two complete balances, one canonical reader                  |
| `kadia_address_activity`   | `addressActivity`   | One strict page, default 25, maximum 100 rows                |

Inputs derive from the SDK's checked-in contract, with tighter adapter limits.
Unknown properties are rejected. There is no arbitrary URL, method, operation or
script input. All tools carry read-only and non-destructive MCP annotations.
The group tool uses `POST /addresses/balances`, an existing **read-only** selection
endpoint; other tools use GET. Selecting addresses does not assert common ownership.

Activity never walks pages automatically. An empty filtered page can have a
continuation; pass **both** `cursor` and `snapshot` and preserve the original
filters. The API validates their binding. On a 409 conflict, discard previous
pages and restart explicitly. Comparison returns all-or-nothing balances, not a
delta inferred from partly loaded rows. Genesis height zero has no block-ID pin.

Successful results preserve the SDK observation in both JSON text and structured
content: `{ "ok": true, "data": ..., "completeness": ..., "status": 200 }`.
Decimal-string ERG/token quantities are never converted to floating point.
Completeness is `complete`, `incomplete`, or `unknown`; retain the response's own
anchors and coverage fields too. Different calls are separate observations.
Unknown inputs, unavailable anchors and missing history must not be treated as zero.

Mining keys are not pool identities; raw vote tuples do not establish approved
protocol parameters. Rent exposure retains its indexed-height anchor and scan
coverage: a complete scan of a partial index is not a complete address balance.
The optional mempool connections cover only transactions returned in that snapshot.
An absent parent is not evidence of a blocked or rejected transaction.

API failures set MCP `isError` and retain HTTP status, problem data, code and
`retry_after`. Local `limit`, `timeout`, `cancelled`, `busy` and `request_failed`
results also set `isError`, without successful or truncated data. Input validation
and unknown methods use the official SDK's protocol/tool error behavior. No request
automatically retries. Server admission errors remain errors, not empty balances.

## Bounds

- Four concurrent HTTP observations; excess calls fail immediately without queuing.
- A 128 KiB stdio receive buffer, checked before JSON parsing.
- Each string is at most 4,096 UTF-8 bytes; group selections have at most 20 entries.
- `KADIA_TIMEOUT_MS`: default 15,000; allowed 100–60,000 milliseconds per observation.
- `KADIA_MAX_RESPONSE_BYTES`: default 524,288; allowed 1,024–2,097,152 bytes per HTTP
  response, checked while streaming and before JSON parsing. Oversize replies fail
  completely. MCP text and structured content each contain the bounded result.
- MCP cancellation and process shutdown abort admitted HTTP requests.

The integration suite spawns the actual stdio server through the official SDK
client and uses local HTTP fixtures. It covers discovery, exact values/coverage,
read-only methods, strict continuation, API errors, streaming limits, timeout,
redirect refusal, concurrency, cancellation and oversized protocol input.

## Primary SDK references

This adapter pins `@modelcontextprotocol/sdk` 1.30.1 and Zod 4.6.5, with transitive
versions and integrity hashes in `package-lock.json`. It uses the supported v1
package API; the separately packaged v2 API has different import paths.
See the official [v1 server guide](https://ts.sdk.modelcontextprotocol.io/server.html)
and [v1 client guide](https://ts.sdk.modelcontextprotocol.io/client.html).
The SDK's installed `StdioServerTransport` exposes `maxBufferSize`; its stdio and
MCP tool APIs are exercised directly by the tests. Dependency upgrades require
rerunning those protocol tests and reviewing the checked-in SDK contract changes.
