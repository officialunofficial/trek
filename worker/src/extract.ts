/**
 * Trek WASM wrapper.
 *
 * On Cloudflare Workers we cannot use the package's default async init
 * (which fetches `trek_rs_bg.wasm` over HTTP at runtime). Instead we import
 * the precompiled WASM module via wrangler's CompiledWasm bundling and pass
 * it to `initSync` once on first use.
 */

// @ts-expect-error - Wrangler bundles `.wasm` imports as `WebAssembly.Module`.
import wasmModule from "../node_modules/@officialunofficial/trek/trek_rs_bg.wasm";
import { initSync, TrekWasm } from "@officialunofficial/trek";

let initialized = false;

function ensureInit(): void {
  if (initialized) return;
  initSync({ module: wasmModule as WebAssembly.Module });
  initialized = true;
}

export interface RunTrekOptions {
  debug?: boolean;
  url?: string | null;
  markdown?: boolean;
  separateMarkdown?: boolean;
  removeExactSelectors?: boolean;
  removePartialSelectors?: boolean;
  [key: string]: unknown;
}

const DEFAULT_OPTIONS: RunTrekOptions = {
  debug: false,
  url: null,
  markdown: false,
  separateMarkdown: false,
  removeExactSelectors: true,
  removePartialSelectors: true,
};

/**
 * Run Trek over an HTML string. `url` is optional but improves extractor
 * selection for site-specific extractors.
 */
export async function runTrek(
  html: string,
  url?: string,
  options?: RunTrekOptions,
): Promise<unknown> {
  ensureInit();

  const merged: RunTrekOptions = {
    ...DEFAULT_OPTIONS,
    ...(options ?? {}),
  };
  if (url && !merged.url) {
    merged.url = url;
  }

  const trek = new TrekWasm(merged);
  try {
    const started = Date.now();
    const result = await trek.parse_async(html);
    const elapsed = Date.now() - started;
    if (result && typeof result === "object" && !("extractTimeMs" in result)) {
      (result as Record<string, unknown>).extractTimeMs = elapsed;
    }
    return result;
  } finally {
    // Free WASM-side memory promptly. parse_async returns a JS-owned object,
    // so this is safe.
    trek.free();
  }
}
