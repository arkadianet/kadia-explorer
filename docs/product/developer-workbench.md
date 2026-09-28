# API playground

`/developers` is a curated, read-only request builder for 19 existing `/v1` endpoints.
It is a practical starting point, not a claim to document every API parameter. The
catalog lives in `frontend/src/lib/developer/playground.ts` alongside field validation.
No new backend endpoint, authentication service or third-party proxy is introduced.

The playground sends only explicit GET requests to the explorer's own origin. Opening
a shared request fills the form but does not execute it. The ordinary site-wide status
poll remains active. Shared links include entered IDs, addresses, filters and continuation
tokens; local saved-address labels are never added. No requests are persisted locally.

The generated cURL example uses POSIX shell quoting and the current explorer host.
Run, cancel, next-page and restart are deliberate controls. Endpoint changes discard
old responses and reset parameters. Editing a form retains the original response path
and disables its continuation until the form matches again.

Each request has a 15-second client timeout, a 2 MiB received-byte limit and no redirects
or cookies. Responses retain the actual HTTP status, elapsed browser time and received
byte count. HTTP problem JSON is inspectable; it is not replaced with an empty success.
The JSON display is capped at 200,000 characters, while download preserves the original
complete response text within the received-byte cap, including exact amount strings.

Strict lists use both returned cursor and snapshot. Only a valid strict pair enables
Next page, and a 409 can be restarted explicitly. Historical balance uses its distinct
height/block-ID contract. Upcoming rent explains its separate `complete` flag. The
field guide and each endpoint explain units, coverage and the interpretation limits.

Original keeps a compact catalog; Prism uses a raised response workspace; Atelier
uses an editorial endpoint index and ledger rules; Aurora uses centered introductions
and rounded catalog/cards. Mobile switches the catalog to a labelled native select.
Tests exercise encoding, sharing, precision, byte limits, continuation, error states,
navigation during a pending response, downloads and all four layout widths.
