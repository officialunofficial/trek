# Track F — Cloudflare Worker for the Trek Playground

Status: Spec / proposed
Owner: TBD
Scope: Add a Cloudflare Worker that serves the static Playground and exposes a public extraction API so the Playground can accept arbitrary URLs without browser CORS hacks.

## 1. Background and motivation

Today the Playground (`playground/index.html`) runs Trek entirely in the browser via `pkg/trek_rs.js` + `pkg/trek_rs_bg.wasm`. To accept a URL the page calls a third-party CORS proxy (`api.allorigins.win`) — fragile, rate-limited, and not under our control. Image previews use the same proxy.

We want a first-party endpoint that:

- Serves the static Playground UI.
- Fetches arbitrary URLs server-side (no browser CORS).
- Runs Trek extraction and returns a `TrekResponse` JSON.
- Costs ~nothing for typical demo traffic.

Cloudflare Workers fits: edge fetch, static assets binding, no infra.

## 2. Architecture options

### Option A — Worker imports `@officialunofficial/trek` and runs WASM in the isolate (RECOMMENDED)

The Worker installs the published npm package, instantiates `TrekWasm`, and runs extraction inside the V8 isolate. The Playground stays static for `GET /`, but URL-driven extraction goes through `GET /api/extract?url=...`.

Pros:
- Single public endpoint. No more `api.allorigins.win`.
- One source of truth for extraction (the Rust crate).
- Works for any caller (curl, scripts, other apps), not just the Playground.
- Caching, rate limiting, observability all colocated.

Cons / risks:
- WASM size matters. Current `pkg/trek_rs_bg.wasm` is **1.8 MB** uncompressed. Workers free tier limits the compressed worker bundle to **3 MB** (post-gzip), paid tier to **10 MB**. WASM compresses well (often 35-50% with gzip/brotli). 1.8 MB → ~700-900 KB compressed should fit free tier; verify in CI before committing to this path. If it ever blows the limit, fall back to Option B (see §11).
- Cold start cost: instantiating a 1.8 MB WASM module adds ~50-150 ms to a cold isolate. Warm requests are near-zero. Acceptable for a demo endpoint.
- The crate currently builds with `wasm-pack --target web`, which emits browser-flavored glue. The Worker runtime accepts ES modules and `WebAssembly.instantiate`, but `--target web` injects a `fetch()`-based loader expecting a URL for the `.wasm` file. Two ways to handle:
  1. Import the `.wasm` directly using Wrangler's WASM module import (`import wasmModule from "@officialunofficial/trek/trek_rs_bg.wasm"`), then call `initSync({ module: wasmModule })`. This is the supported path.
  2. Add a second wasm-pack output target (`--target bundler` or a Worker-specific dist) to `pkg/`. Simpler ergonomics, but adds a build step and a second published artifact.
  Recommendation: start with (1) — no changes to the publish pipeline. If it gets ugly, add a `worker` output later.

### Option B — Worker as a CORS-stripping fetch proxy only

`GET /api/fetch?url=...` returns the raw HTML; the browser still runs Trek WASM. Smaller worker, no WASM in the isolate.

Pros:
- Tiny worker (<50 KB). Always within free tier.
- No WASM cold-start.
- The Playground already has the WASM init path; minimal client changes.

Cons:
- Doesn't give us a real public extraction API. Anyone wanting JSON has to also load the WASM.
- Still need SSRF protection, body cap, timeout — same security surface as Option A but less value.

### Recommendation

**Ship Option A.** It's the right shape: one endpoint that does the whole job, reusable beyond the Playground. Option B exists as a fallback if Option A's bundle ever exceeds the size limit. Both can coexist (`/api/fetch` and `/api/extract`) if we want the browser-side path too.

## 3. Worker route shape

| Method | Path | Behavior |
|---|---|---|
| GET | `/` | Serve `playground/index.html` via the assets binding. |
| GET | `/playground/*`, `/pkg/*`, `/trek.svg`, etc. | Static assets passthrough. |
| GET | `/api/health` | `200 {"ok":true,"version":"<pkg version>"}`. No CORS preflight needed. |
| GET | `/api/extract?url=<URL>` | Fetch URL server-side, run Trek, return `TrekResponse` JSON. |
| POST | `/api/extract` | Body `{ html: string, url?: string, options?: TrekOptions }`. Run Trek on supplied HTML. |
| OPTIONS | `/api/*` | CORS preflight. Returns 204 with the standard headers. |

Response shape for `/api/extract` mirrors `TrekResponse` from `src/types.rs` exactly — same JSON the WASM `parse()` returns today, so the Playground's display code keeps working unchanged.

Error shape: `{ "error": "<short code>", "message": "<human>" }` with appropriate status (400 invalid URL, 413 body too large, 415 non-HTML, 422 fetch failed, 504 fetch timeout, 500 extraction failed).

## 4. Directory layout

New `worker/` directory at repo root:

```
worker/
├── wrangler.jsonc          # Worker config (name, main, compat date, assets binding)
├── package.json            # deps: @officialunofficial/trek, wrangler (dev), typescript (dev)
├── tsconfig.json           # ES2022, moduleResolution: bundler, types: ["@cloudflare/workers-types"]
├── src/
│   ├── index.ts            # Worker entry: route dispatch, CORS, asset fallthrough
│   ├── extract.ts          # Trek WASM init (lazy, module-scoped), extract() helper
│   ├── fetch-url.ts        # Safe URL fetcher: SSRF guard, timeout, body cap, content-type check
│   └── cors.ts             # CORS header helper + preflight handler
└── README.md               # Local dev + deploy instructions
```

Why a sibling dir, not a workspace? The Trek crate is Rust + wasm-pack; the Worker is TS + npm. Keeping them adjacent but unrelated avoids npm-workspace-vs-cargo-workspace tangles. The Worker depends on the published npm artifact, not on local `pkg/`, so it builds independently.

## 5. `wrangler.jsonc` (sample)

```jsonc
{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "trek-playground",
  "main": "src/index.ts",
  "compatibility_date": "2025-04-01",
  "compatibility_flags": ["nodejs_compat"],
  "observability": { "enabled": true },
  "assets": {
    "directory": "../playground",
    "binding": "ASSETS",
    "not_found_handling": "single-page-application"
  },
  "limits": { "cpu_ms": 30000 },
  "placement": { "mode": "smart" }
}
```

Notes:
- `assets.directory` points at the existing `playground/` so we don't duplicate files. The Worker serves the playground's HTML, the `pkg/` JS+WASM bundle (which the Playground imports from `../pkg/trek_rs.js`), and the SVG.
  - Caveat: the Playground references `../pkg/trek_rs.js`. We either (a) move/copy the published `pkg/` under `playground/pkg/` for asset serving, (b) add a build step in `make worker-deploy` that copies `pkg/` into a temp asset dir, or (c) update the Playground to import from a CDN (`https://esm.sh/@officialunofficial/trek`) — option (c) is simplest and removes the local-build coupling.
- `nodejs_compat` is on so the Trek npm package's loader can use any Node-style globals if needed (it likely doesn't, but cheap insurance).
- `cpu_ms: 30000` is the paid-tier max. Free tier is capped at 10 ms CPU per request — Trek extraction on a typical article is well under that, but big pages may need the bump. Verify on real traffic before downgrading.
- No KV, D1, R2, AI, or queue bindings. None needed for v1.

## 6. Worker entry sketch

```ts
// src/index.ts
import { handleExtract } from "./extract";
import { withCors, preflight } from "./cors";

export interface Env {
  ASSETS: Fetcher;
}

export default {
  async fetch(req: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(req.url);

    if (req.method === "OPTIONS" && url.pathname.startsWith("/api/")) {
      return preflight();
    }

    if (url.pathname === "/api/health") {
      return withCors(Response.json({ ok: true, version: "0.2.0" }));
    }

    if (url.pathname === "/api/extract") {
      try {
        const res = await handleExtract(req, ctx);
        return withCors(res);
      } catch (err) {
        return withCors(Response.json(
          { error: "internal", message: String(err) },
          { status: 500 }
        ));
      }
    }

    // Fallthrough: static playground assets
    return env.ASSETS.fetch(req);
  },
};
```

```ts
// src/extract.ts (sketch)
import init, { TrekWasm } from "@officialunofficial/trek";
import wasmModule from "@officialunofficial/trek/trek_rs_bg.wasm";
import { safeFetch } from "./fetch-url";

let ready: Promise<void> | null = null;
function ensureInit() {
  if (!ready) ready = (async () => { await init(wasmModule); })();
  return ready;
}

export async function handleExtract(req: Request, ctx: ExecutionContext) {
  await ensureInit();
  const url = new URL(req.url);

  let html: string;
  let sourceUrl: string | undefined;
  let options: Record<string, unknown> = { removeExactSelectors: true, removePartialSelectors: true };

  if (req.method === "GET") {
    sourceUrl = url.searchParams.get("url") ?? undefined;
    if (!sourceUrl) return Response.json({ error: "missing_url" }, { status: 400 });
    html = await safeFetch(sourceUrl);
    options.url = sourceUrl;
  } else if (req.method === "POST") {
    const body = await req.json<{ html: string; url?: string; options?: Record<string, unknown> }>();
    if (!body?.html) return Response.json({ error: "missing_html" }, { status: 400 });
    html = body.html;
    sourceUrl = body.url;
    options = { ...options, ...(body.options ?? {}), url: sourceUrl };
  } else {
    return new Response("Method Not Allowed", { status: 405 });
  }

  const trek = new TrekWasm(options);
  try {
    const result = await trek.parse_async(html);
    return Response.json(result);
  } finally {
    trek.free();
  }
}
```

The `import wasmModule from "@officialunofficial/trek/trek_rs_bg.wasm"` syntax is Wrangler's convention for shipping `.wasm` alongside JS. If the published package's `files` array doesn't include the `.wasm` it already does (we checked: `pkg/package.json` lists `trek_rs_bg.wasm`), so this works out of the box.

## 7. Security

The Worker fetches arbitrary URLs on behalf of the caller. That's an SSRF surface. Mitigations:

- **Scheme allowlist**: only `http:` and `https:`.
- **Host blocklist**: reject if the resolved host is `localhost`, `127.0.0.0/8`, `0.0.0.0`, `169.254.0.0/16` (link-local / metadata), `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `::1`, `fc00::/7`, `fe80::/10`. Workers' fetch already blocks some of these but not all — implement defense in depth.
- **DNS pinning**: in `safeFetch`, parse the URL host, reject string-form private IPs early. (Workers can't resolve DNS to check post-resolution IP, so we rely on the host string + the runtime's own protections.)
- **Body size cap**: stream the response, abort after **5 MB**. Prevents memory blowup from giant pages.
- **Timeout**: 10 s on the upstream fetch via `AbortSignal.timeout(10_000)`.
- **Content-Type filter**: only accept `text/html`, `application/xhtml+xml`, or `text/plain`. Reject binary types early with 415.
- **User-Agent**: send `User-Agent: TrekBot/0.2 (+https://github.com/officialunofficial/trek)` so we're identifiable and respectful.
- **Rate limiting**: rely on Cloudflare's free WAF rate-limiting rules at the zone level — set "10 req / 10 s per IP on `/api/extract`" via the dashboard. No code changes needed for v1. If we outgrow that, add a Durable Object or Workers Rate Limiting binding.
- **Input size cap on POST**: reject bodies > **2 MB** (large enough for any sane HTML paste).

No allowlist of target hostnames in v1 — the demo's whole point is "throw any URL at it". Revisit if abuse appears.

## 8. CORS

API routes return:

```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type
Access-Control-Max-Age: 86400
```

`*` is fine for v1 — the API has no auth and no cookies. If we ever add auth, switch to a reflected `Origin` allowlist.

Static asset responses don't need CORS headers (same-origin from the Playground's perspective).

## 9. Playground integration

Changes to `playground/index.html`:

1. Add a single `API_BASE` constant near the top of the script: `const API_BASE = window.location.origin.includes("localhost") ? "http://localhost:8787" : "";` (empty string = same origin in production).
2. Replace the body of `window.fetchUrl` to call `${API_BASE}/api/extract?url=...` instead of `api.allorigins.win`. The response is already a parsed `TrekResponse` — skip the local WASM call entirely on this path. Set `lastResult` from the JSON response and call `displayResults(currentTab)` directly.
3. Keep the existing paste-HTML flow (`window.extractContent`) unchanged — it still uses the in-browser WASM and works offline.
4. Image proxying: today every image goes through `api.allorigins.win`. Optional v1.1: add a `/api/img?url=...` that streams remote images with a 5 MB cap and proper `Cache-Control`. Out of scope for the initial worker.
5. Add a small UI hint when the API path fails (e.g. 504): "URL fetch timed out — try pasting the HTML instead."

The `pkg/trek_rs.js` import stays. Browsers loading the static page directly (e.g. via `make playground`) get the local WASM path; visitors of the deployed Worker get the API path. Same UI.

## 10. Local development

Two processes during dev:

```bash
# Terminal 1 — build WASM and serve playground locally (existing flow)
make playground   # python serve.py on :8000, plus wasm-build

# Terminal 2 — Worker on :8787 with hot reload
cd worker && npx wrangler dev
```

For end-to-end testing of the full Worker (assets + API) without `make serve`:

```bash
cd worker && npx wrangler dev   # serves both assets and /api/* on :8787
```

Add a Make target:

```makefile
.PHONY: worker-dev
worker-dev: ## Run the Cloudflare Worker locally (assets + API on :8787)
	cd worker && npm install && npx wrangler dev
```

`wrangler dev` reads `wrangler.jsonc`, mounts `../playground/` as the assets dir, and serves `/api/*` from `src/index.ts` with file-watch reload. WASM is loaded from the npm package in `worker/node_modules/`.

## 11. Deploy

Make target:

```makefile
.PHONY: worker-deploy
worker-deploy: wasm-build ## Deploy the Cloudflare Worker (and update playground assets)
	cd worker && npm install && npx wrangler deploy
```

Prereqs:
- `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` in the deployer's env (or `wrangler login` for interactive use).
- No secrets needed for v1. (No DB, no auth, no third-party API keys.)

Custom domain: configure in the Cloudflare dashboard under Worker > Triggers > Custom Domains. Suggested: `trek.unofficial.run` or similar. The Worker is the origin — no separate hosting needed.

## 12. CI

Add a GitHub Actions stub `.github/workflows/worker.yml` that, on PRs touching `worker/**` or `playground/**` or `pkg/**`:

1. `npm ci` in `worker/`.
2. `npx wrangler deploy --dry-run --outdir dist` — exercises the bundle pipeline without publishing.
3. Check the resulting `dist/index.js` size; fail if > 2.8 MB compressed (free tier safety margin).
4. Optional: `tsc --noEmit` for type-checking.

No deploy on merge in v1 — keep deploy manual via `make worker-deploy` until we trust the path. Add auto-deploy-on-main later via `cloudflare/wrangler-action@v3`.

## 13. Bundle size escape hatch

If at any point the Worker bundle (gzipped) exceeds the 3 MB free-tier or 10 MB paid limit:

1. **Short term**: turn on paid (currently $5/mo includes 10 MB). Buys headroom while we shrink the WASM.
2. **Medium term**: shrink the WASM. Trek already builds with `opt-level = "z"`. Easy wins: strip more debug info (`wasm-strip`), enable `wasm-opt -Oz` (we currently pass `--no-opt`), audit pulled-in serde features.
3. **Fallback**: switch to Option B from §2. The Worker becomes a fetch+CORS proxy only; the WASM stays in the browser. Playground client gets a tiny refactor: `fetchUrl` calls `/api/fetch` to get HTML, then runs the existing local extraction. We lose the public extraction API but keep the demo working.

The decision tree should be checked at deploy time: the Wrangler dry-run reports compressed size — if it's > 2.5 MB, file an issue and decide.

## 14. File list

| File | Purpose |
|---|---|
| `worker/wrangler.jsonc` | Worker config: name, main, compat date, assets binding pointing at `../playground/`, observability on. |
| `worker/package.json` | Declares deps on `@officialunofficial/trek` (runtime) and `wrangler`, `typescript`, `@cloudflare/workers-types` (dev). |
| `worker/tsconfig.json` | ES2022 + bundler resolution + `@cloudflare/workers-types`. |
| `worker/src/index.ts` | Entry. Routes `/api/health`, `/api/extract`, OPTIONS preflight; falls through to `env.ASSETS.fetch`. |
| `worker/src/extract.ts` | Lazy-init Trek WASM module, `handleExtract(req)` for GET (URL fetch) and POST (HTML body). |
| `worker/src/fetch-url.ts` | `safeFetch(url)`: scheme + private-IP guard, 10 s timeout, 5 MB cap, content-type filter, identifying UA. |
| `worker/src/cors.ts` | `withCors(res)` and `preflight()` helpers. |
| `worker/README.md` | Quickstart: `npm install && npx wrangler dev`, deploy instructions, env var notes. |
| `Makefile` (edit) | Add `worker-dev` and `worker-deploy` targets. |
| `playground/index.html` (edit) | Add `API_BASE`, swap `fetchUrl` to call `/api/extract?url=...`, keep paste-HTML path on local WASM. |
| `.github/workflows/worker.yml` (new) | CI: install + `wrangler deploy --dry-run` + bundle size check on PRs touching worker/playground/pkg. |
| `docs/refactor/track-f-worker.md` | This spec. |

## 15. Open questions for implementation

- Asset coupling for `pkg/`: do we (a) commit `pkg/` into Git so Workers Assets can serve it from `../pkg/`, (b) copy `pkg/` to a temp dir at deploy time, or (c) move the Playground's import to `https://esm.sh/@officialunofficial/trek`? Recommendation: (c) for the deployed Worker, keep local `pkg/` for `make playground`. One-line conditional in the Playground.
- Should `/api/extract` also accept `Accept: text/html` and return rendered article HTML directly (Reader-Mode-as-a-Service)? Useful but out of v1 scope; flag for v1.1.
- Caching: Cloudflare's edge cache will cache `GET /api/extract?url=...` responses by default if we set `Cache-Control: public, max-age=300, s-maxage=3600`. Worth adding in v1 — extraction is deterministic for a given URL+options snapshot. Use `cache.match` / `cache.put` explicitly to be sure.
- Telemetry: with `observability.enabled = true` we get tail logs and analytics for free. No need for custom metrics in v1.
