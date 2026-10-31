export async function sha256Hex(message: string): Promise<string> {
  const msgUint8 = new TextEncoder().encode(message);
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const hashBuffer = await crypto.subtle.digest('SHA-256', msgUint8);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
  }
  // Fallback simple 64-char pseudo-hash if subtle crypto unavailable
  let h1 = 0xdeadbeef, h2 = 0x41c6ce57;
  for (let i = 0; i < message.length; i++) {
    const ch = message.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  const s1 = (h1 >>> 0).toString(16).padStart(8, '0');
  const s2 = (h2 >>> 0).toString(16).padStart(8, '0');
  return (s1 + s2).repeat(4);
}

export const GENESIS_HASH = '0000000000000000000000000000000000000000000000000000000000000000';

export async function computeAuditEntryHash(
  prevHash: string,
  seq: number,
  action: string,
  topic: string,
  source: string,
  target: string,
  version: number,
  outcome: string,
  reason: string
): Promise<string> {
  const payload = `${prevHash}:${seq}:${action}:${topic}:${source}:${target}:${version}:${outcome}:${reason}`;
  return sha256Hex(payload);
}
