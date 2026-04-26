/**
 * Trek Worker — wraps the Trek WASM library and serves the Playground.
 *
 * Routes:
 *   OPTIONS *               — CORS preflight
 *   GET  /api/health        — liveness, returns { ok, version }
 *   GET  /api/extract?url=  — fetch URL, run Trek, return JSON
 *   POST /api/extract       — body { html, url?, options? }, run Trek
 *   *                       — fall through to ASSETS (the Playground)
 */

import { runTrek, type RunTrekOptions } from "./extract.js";
import { validateOutboundUrl } from "./security.js";

// Pulled from the bundled @officialunofficial/trek package.
import trekPkg from "../node_modules/@officialunofficial/trek/package.json";

interface Env {
  ASSETS: Fetcher;
}

const TREK_VERSION: string = (trekPkg as { version?: string }).version ?? "unknown";

const BODY_CAP_BYTES = 5 * 1024 * 1024; // 5 MB
const FETCH_TIMEOUT_MS = 10_000;

const CORS_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
  "Access-Control-Max-Age": "86400",
};

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), {
    ...init,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      ...CORS_HEADERS,
      ...(init.headers as Record<string, string> | undefined),
    },
  });
}

function errorResponse(status: number, message: string, detail?: string): Response {
  return jsonResponse(
    { error: message, ...(detail ? { detail } : {}) },
    { status },
  );
}

/**
 * Read up to `cap` bytes from a Response body. Throws if the stream exceeds
 * the cap so we never buffer unbounded data into memory.
 */
async function readCapped(response: Response, cap: number): Promise<string> {
  const reader = response.body?.getReader();
  if (!reader) return "";
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > cap) {
      try {
        await reader.cancel();
      } catch {
        // Ignore cancel failures.
      }
      throw new Error(`response exceeded ${cap} byte cap`);
    }
    chunks.push(value);
  }
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  // Trek expects HTML text. Use TextDecoder with replacement on bad bytes.
  return new TextDecoder("utf-8", { fatal: false, ignoreBOM: false }).decode(
    merged,
  );
}

async function fetchHtml(url: string): Promise<{ html: string; finalUrl: string }> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  try {
    const upstream = await fetch(url, {
      method: "GET",
      redirect: "follow",
      signal: controller.signal,
      headers: {
        // Match a generic browser UA so most CDNs let us in.
        "User-Agent":
          "Mozilla/5.0 (compatible; TrekWorker/0.1; +https://github.com/officialunofficial/trek)",
        Accept:
          "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "Accept-Language": "en",
      },
    });
    if (!upstream.ok) {
      throw new Error(`upstream ${upstream.status} ${upstream.statusText}`);
    }
    const html = await readCapped(upstream, BODY_CAP_BYTES);
    return { html, finalUrl: upstream.url || url };
  } finally {
    clearTimeout(timer);
  }
}

async function handleHealth(): Promise<Response> {
  return jsonResponse({ ok: true, version: TREK_VERSION });
}

async function handleGetExtract(request: Request): Promise<Response> {
  const url = new URL(request.url).searchParams.get("url");
  if (!url) {
    return errorResponse(400, "missing required query parameter: url");
  }
  const validation = validateOutboundUrl(url);
  if (!validation.ok) {
    return errorResponse(400, "url rejected", validation.reason);
  }

  let html: string;
  let finalUrl: string;
  try {
    const fetched = await fetchHtml(url);
    html = fetched.html;
    finalUrl = fetched.finalUrl;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (message.includes("aborted") || message.includes("timeout")) {
      return errorResponse(504, "upstream fetch timed out", message);
    }
    if (message.includes("byte cap")) {
      return errorResponse(413, "upstream body too large", message);
    }
    return errorResponse(502, "upstream fetch failed", message);
  }

  try {
    const result = await runTrek(html, finalUrl);
    return jsonResponse(result);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return errorResponse(500, "extraction failed", message);
  }
}

async function readJsonCapped(request: Request): Promise<unknown> {
  const lengthHeader = request.headers.get("Content-Length");
  if (lengthHeader) {
    const len = Number(lengthHeader);
    if (Number.isFinite(len) && len > BODY_CAP_BYTES) {
      throw new Error(`request body exceeds ${BODY_CAP_BYTES} byte cap`);
    }
  }
  // Even when Content-Length is missing/lying, cap manually.
  const reader = request.body?.getReader();
  if (!reader) return {};
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > BODY_CAP_BYTES) {
      try {
        await reader.cancel();
      } catch {
        // Ignore.
      }
      throw new Error(`request body exceeds ${BODY_CAP_BYTES} byte cap`);
    }
    chunks.push(value);
  }
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  const text = new TextDecoder("utf-8", {
    fatal: false,
    ignoreBOM: false,
  }).decode(merged);
  if (text.length === 0) return {};
  try {
    return JSON.parse(text);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new Error(`invalid JSON body: ${message}`);
  }
}

async function handlePostExtract(request: Request): Promise<Response> {
  let payload: unknown;
  try {
    payload = await readJsonCapped(request);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (message.includes("byte cap")) {
      return errorResponse(413, "request body too large", message);
    }
    return errorResponse(400, "could not read request body", message);
  }

  if (!payload || typeof payload !== "object") {
    return errorResponse(400, "request body must be a JSON object");
  }
  const { html, url, options } = payload as {
    html?: unknown;
    url?: unknown;
    options?: unknown;
  };
  if (typeof html !== "string" || html.length === 0) {
    return errorResponse(400, "missing required field: html (string)");
  }
  if (url !== undefined && typeof url !== "string") {
    return errorResponse(400, "field url must be a string when provided");
  }
  if (
    options !== undefined &&
    (options === null || typeof options !== "object" || Array.isArray(options))
  ) {
    return errorResponse(
      400,
      "field options must be an object when provided",
    );
  }

  try {
    const result = await runTrek(
      html,
      typeof url === "string" ? url : undefined,
      options as RunTrekOptions | undefined,
    );
    return jsonResponse(result);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return errorResponse(500, "extraction failed", message);
  }
}

const worker: ExportedHandler<Env> = {
  async fetch(request, env, _ctx): Promise<Response> {
    const url = new URL(request.url);

    // CORS preflight applies to any path.
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    if (url.pathname === "/api/health" && request.method === "GET") {
      return handleHealth();
    }
    if (url.pathname === "/api/extract") {
      if (request.method === "GET") return handleGetExtract(request);
      if (request.method === "POST") return handlePostExtract(request);
      return errorResponse(405, "method not allowed");
    }
    if (url.pathname.startsWith("/api/")) {
      return errorResponse(404, `no such route: ${url.pathname}`);
    }

    // Everything else — playground assets.
    return env.ASSETS.fetch(request);
  },
};

export default worker;
