# Trek Worker

A Cloudflare Worker that wraps the Trek WASM extractor and serves the Trek
Playground (`../playground/`) as static assets.

## Routes

| Method | Path | Description |
|--------|------|-------------|
| `OPTIONS` | `*` | CORS preflight (`Access-Control-Allow-Origin: *`). |
| `GET` | `/api/health` | `{ ok: true, version }`. |
| `GET` | `/api/extract?url=<URL>` | Fetches `URL` (10 s timeout, 5 MB cap), runs Trek, returns the result as JSON. |
| `POST` | `/api/extract` | Body `{ html, url?, options? }` (5 MB cap), runs Trek, returns JSON. |
| any other path | falls through to `ASSETS` (the Playground). |

All API responses include CORS headers so the Playground (or any other
client) can call them cross-origin.

## SSRF guard

The worker's outbound URL fetch (the `GET /api/extract?url=` flow) goes
through a small validator in `src/security.ts`:

- Only `http:` and `https:` are allowed.
- Hostnames matching `localhost`, `*.local`, `*.internal`, `*.lan` are
  rejected.
- IPv4 literals in `0.0.0.0/8`, `10/8`, `127/8`, `169.254/16`, `172.16/12`,
  `192.168/16`, `100.64/10` (CGNAT), `224/4` (multicast), `240/4` (reserved)
  are rejected.
- IPv6 literals `::1`, `::`, `fc00::/7`, `fe80::/10`, `ff00::/8`, plus
  IPv4-mapped forms of any blocked IPv4 are rejected.

DNS rebinding cannot be fully prevented from a Worker (we cannot resolve
DNS), so this is a best-effort literal-IP guard. If you start exposing this
worker to untrusted callers, also restrict it via WAF or per-token allow
lists.

## Develop

```bash
cd worker
npm install
npx wrangler dev
```

`wrangler dev` will start a local server (default port 8787) that serves the
playground and API routes. `Cmd-click` the printed URL to open it.

## Deploy

```bash
cd worker
npx wrangler deploy           # actual deploy (needs CF auth)
npx wrangler deploy --dry-run --outdir=dist   # validate + bundle only
```

The Makefile in the repo root also exposes `make worker-dev`,
`make worker-deploy`, and `make worker-deploy-dry`.

## Bundle size

The Trek WASM is ~1.8 MB raw and roughly ~700 KB compressed. Cloudflare's
free Workers plan limits a single Worker to **3 MB compressed**; paid plans
allow 10 MB. The bundle is well under both, but check the `wrangler deploy
--dry-run` output before each release — if a future Trek release pushes the
bundle past 3 MB compressed, the deploy will fail on the free plan and we
will need to fall back to either:

- Splitting the playground out to Cloudflare Pages (Option B in the spec),
  with the worker only serving the API; or
- Moving to a paid Workers plan.

## Environment

No secrets or bindings are required for the basic flow. The worker uses:

- `ASSETS` — Workers Static Assets binding pointing at `../playground/`,
  configured in `wrangler.jsonc`.

If you later add per-domain rate limiting or a private allowlist token,
configure them via `wrangler secret put`.
