# The reports' relay

Artoo's Report a problem (Help → Report a Problem…, ⌥⌘/) sends a report here. This Cloudflare Worker files it as an
issue in a private intake repository, as a GitHub App. The app holds no key: the App's key is a secret of this Worker
alone. When the relay can't be reached, or is closed, the app opens GitHub's new-issue page of the project, filled in.

What it does with a report:

- checks it (a title, sizes, a PNG picture) and takes out again what is private (home paths, tokens, e-mail
  addresses), and turns `@mentions` into text so nobody is notified;
- counts it: 10 a day per installation, 30 per address, `DAILY_CAP` in all (429 past them);
- files it as an issue labelled `from-app` (and `agent` when an agent drafted it), the window's picture kept in the
  intake under `attachments/`; the same problem again, while the first is still open in the intake, becomes a
  comment on it (by the app's fingerprint);
- answers the app with the report's id (`R-7F3A2C`), and later, for the installation that sent it only, what became
  of it: received, moved to the project (with its address), a duplicate, fixed, closed.

A report is made public by moving its issue to the project: on the issue, **Transfer issue** → `artoo`.
Check the text first; the picture stays in the private intake.

## API

```
POST /v1/report              {install, version, commit, title, body, fingerprint, agent, picture?}
                             → 200 {id} · 400 not a report · 413 too large · 429 too many · 503 closed
GET  /v1/report/<id>?install → 200 {state, url} · 404
```

## Setting it up (once)

1. **The intake repository**: a private `Thernal/artoo-reports` with a README (so `main` exists), and the labels
   `from-app`, `agent` and `duplicate`.
2. **The GitHub App** (Settings → Developer settings → GitHub Apps → New): no webhook; repository permissions
   **Issues: read and write**, **Contents: read and write** (the pictures), Metadata: read. Install it on
   `artoo-reports` only. Note its App ID, and generate a private key.
3. **The key as PKCS#8** (the Worker's crypto reads only that):
   `openssl pkcs8 -topk8 -inform PEM -outform PEM -nocrypt -in app.private-key.pem -out app-pkcs8.pem`
4. **Cloudflare**: `npx wrangler login`, then from this folder
   `npx wrangler kv namespace create REPORTS` — put the printed id in `wrangler.toml`;
   `npx wrangler secret put APP_ID`, `npx wrangler secret put APP_KEY < app-pkcs8.pem`; `npx wrangler deploy`.
   Delete both `.pem` files afterwards.
5. **The app**: the address `wrangler deploy` printed goes in `RELAY` (`crates/reports/src/lib.rs`), or in
   `ARTOO_REPORT_URL` to try it first.

Closing the reports: `OPEN = "0"` in `wrangler.toml` and deploy (the app then offers GitHub's page). A key that
leaked: revoke it in the App's settings and put a new one with `wrangler secret put APP_KEY` — the app needs no update.

## GitLance's address

GitLance 1.3.0 sends its reports to `gitlance-reports.h-humbat0.workers.dev`, the relay's address before the name
changed. `forward/` keeps that address: a Worker named `gitlance-reports` that hands every request to this one
(`artoo-reports`) through a service binding, so GitLance's reports land in the same intake and their states are read
from the same KV. It holds no key. Deploy it after this one: `cd forward && npx wrangler deploy`.

How the name moved (2026-11-08): the Worker that was `gitlance-reports` was renamed `artoo-reports` (Cloudflare's API,
`PATCH /accounts/<account>/workers/workers/<id>` with the new `name`). A rename keeps a Worker's secrets and bindings,
so the App's key went with it and none was made again; the old address answers 404 from that moment until `forward/`
is deployed under it, a few seconds.

It goes once Artoo 2.0.0 is out (#173): `npx wrangler delete --name gitlance-reports`. GitLance then opens GitHub's
page instead, as it does whenever the relay can't be reached.

## Tests

`npm test` (Node 20+): the checks, the scrubbing, the issue's text, the App's JWT, the Worker against a GitHub and a
KV in memory, and GitLance's address handing a request on.
