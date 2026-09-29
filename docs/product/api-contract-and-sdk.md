# Versioned workflow API contract and portable client

`/openapi.json` is an OpenAPI 3.1.1 contract for 25 documented read operations,
including the read-only group-balances POST. It explicitly covers a subset of the
server API, not every operational or legacy endpoint. Contract version 1.0.0 is
independent of chain height, database schema and release version.

The subset includes bounded mining/header observations (`mining`) and compact
address rent evidence (`addressRentExposure`, requiring `view: "exposure"`). Mining
ranges are inclusive and limited to 20,160 headers, with an optional canonical end
block pin. Rent context distinguishes scan completeness from full chain coverage.
Mempool connections are optional for compatibility with older API deployments and
cover only returned transactions. Preserve these fields when interpreting results.

The generator at `frontend/scripts/api-contract.mjs` builds response schemas and
portable TypeScript declarations from the explorer's wire types. Request paths and
parameter rules live in the adjacent catalog. `npm run contract:check` rejects stale
generated files and is part of the ordinary frontend check gate. Unit tests also
check that documented paths exist in the Rust router. These checks do not replace
backend integration tests or imply runtime JSON Schema validation by the SDK.

## Using the client

Download `kadia-client.js`, `operations.js`, `kadia-client.d.ts` and `api-types.d.ts`
from `/sdk/` into one directory. These are portable ES modules, not a published npm
package. Browser ESM and Node 22+ can import `createKadiaClient` from the JavaScript
file; TypeScript resolves the adjacent declarations. An executable Node example is
provided at `/sdk/inspect-transaction.mjs`.

```ts
import { createKadiaClient } from './kadia-client.js';

const client = createKadiaClient({ baseUrl: 'https://explorer.kadia.io/v1' });
const result = await client.observe('address', { addr: address });
console.log(result.completeness, result.data.balance.nano);
// Monetary values stay strings. Convert to BigInt only when doing integer arithmetic.
```

`observe` retains the store-completeness header. An absent/unexposed header is
`unknown`, not complete. `request` returns only the typed response body. Browsers
calling another origin can only read headers that its CORS policy exposes; same-origin
calls and Node can read the available header directly. No helper infers ownership,
circulating supply or network-wide transaction acceptance.

`pages` supports only the listed strict pageable routes. It forwards both opaque
continuation fields, checks the anchor, accepts an empty page with continuation,
rejects repeated cursors and stops at configured page/item limits. A later error or
limit means already-yielded pages are incomplete. A 409 is surfaced to the caller;
the client never silently restarts or merges two snapshots. Historical box pages use
their separate height/block contract and are requested explicitly instead.

All calls omit cookies/referrer and reject redirects. The base URL must be explicit
and has no embedded credentials. Default limits are 15 seconds and 2 MiB per response;
the hard configurable ceilings are 60 seconds and 8 MiB. Request options accept an
AbortSignal. Problem details preserve HTTP status, optional code and Retry-After.
There is no automatic retry, polling, background subscription or credential storage.

Regenerate after changing supported wire types or the request catalog. Contract
changes that remove fields, change meanings/types or require new inputs need a
version change and migration note. Additive fields remain allowed. The pinned format
is documented by the [OpenAPI 3.1.1 specification](https://spec.openapis.org/oas/v3.1.1.html).
