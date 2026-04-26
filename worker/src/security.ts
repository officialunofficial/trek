/**
 * SSRF guard for outbound URL fetches.
 *
 * Cloudflare Workers cannot resolve DNS, so we cannot defeat all DNS-rebinding
 * attacks. We do the next best thing: a literal-IP and well-known-suffix
 * blocklist plus a protocol allowlist. This stops the easy mistakes
 * (`http://localhost`, `http://10.0.0.1`, `http://printer.local`) without
 * pretending to be perfect.
 */

const PRIVATE_HOSTNAMES = new Set([
  "localhost",
  "ip6-localhost",
  "ip6-loopback",
  "broadcasthost",
]);

const BLOCKED_SUFFIXES = [".local", ".internal", ".lan", ".localhost"];

/**
 * Returns true if `host` is an IPv4 literal in a private/reserved range.
 */
function isPrivateIPv4(host: string): boolean {
  const parts = host.split(".");
  if (parts.length !== 4) return false;
  const octets = parts.map((p) => Number(p));
  if (octets.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) {
    return false;
  }
  const [a, b] = octets as [number, number, number, number];

  // 0.0.0.0/8 - "this network"
  if (a === 0) return true;
  // 10.0.0.0/8 - private
  if (a === 10) return true;
  // 127.0.0.0/8 - loopback
  if (a === 127) return true;
  // 169.254.0.0/16 - link-local
  if (a === 169 && b === 254) return true;
  // 172.16.0.0/12 - private
  if (a === 172 && b >= 16 && b <= 31) return true;
  // 192.168.0.0/16 - private
  if (a === 192 && b === 168) return true;
  // 100.64.0.0/10 - CGNAT
  if (a === 100 && b >= 64 && b <= 127) return true;
  // 224.0.0.0/4 - multicast
  if (a >= 224 && a <= 239) return true;
  // 240.0.0.0/4 - reserved
  if (a >= 240) return true;
  return false;
}

/**
 * Returns true if `host` is an IPv6 literal in a private/reserved range.
 * `host` must be the bracket-stripped form (e.g. "::1", not "[::1]").
 */
function isPrivateIPv6(host: string): boolean {
  const lowered = host.toLowerCase().trim();
  if (lowered === "::1" || lowered === "::") return true;
  // IPv4-mapped IPv6: ::ffff:a.b.c.d
  const mapped = lowered.match(/^::ffff:(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})$/);
  if (mapped && mapped[1]) return isPrivateIPv4(mapped[1]);
  // fc00::/7 - unique local
  if (/^f[cd][0-9a-f]{0,2}:/.test(lowered)) return true;
  // fe80::/10 - link-local
  if (/^fe[89ab][0-9a-f]?:/.test(lowered)) return true;
  // ff00::/8 - multicast
  if (/^ff[0-9a-f]{0,2}:/.test(lowered)) return true;
  return false;
}

export interface UrlValidationResult {
  ok: boolean;
  reason?: string;
}

/**
 * Validate that a URL is safe to fetch from a Worker. Rejects non-HTTP(S)
 * schemes and hosts that look like loopback / private / link-local / internal.
 */
export function validateOutboundUrl(raw: string): UrlValidationResult {
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return { ok: false, reason: "invalid URL" };
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return { ok: false, reason: `protocol ${parsed.protocol} not allowed` };
  }

  // Strip IPv6 brackets if present.
  let host = parsed.hostname;
  if (host.startsWith("[") && host.endsWith("]")) {
    host = host.slice(1, -1);
  }
  host = host.toLowerCase();

  if (host.length === 0) {
    return { ok: false, reason: "empty hostname" };
  }

  if (PRIVATE_HOSTNAMES.has(host)) {
    return { ok: false, reason: `hostname ${host} is blocked` };
  }

  for (const suffix of BLOCKED_SUFFIXES) {
    if (host === suffix.slice(1) || host.endsWith(suffix)) {
      return { ok: false, reason: `hostname suffix ${suffix} is blocked` };
    }
  }

  // IPv6 literal
  if (host.includes(":")) {
    if (isPrivateIPv6(host)) {
      return { ok: false, reason: `IPv6 ${host} is in a private range` };
    }
  } else if (/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host)) {
    if (isPrivateIPv4(host)) {
      return { ok: false, reason: `IPv4 ${host} is in a private range` };
    }
  }

  return { ok: true };
}
