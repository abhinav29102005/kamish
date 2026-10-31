export async function sha256Hex(message: string): Promise<string> {
  const msgUint8 = new TextEncoder().encode(message);
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const hashBuffer = await crypto.subtle.digest('SHA-256', msgUint8);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
  }
  throw new Error('WebCrypto is unavailable; demo audit hashing cannot be performed');
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
  const payload = JSON.stringify(['kamish-demo-audit-v2', prevHash, seq, action, topic, source, target, version, outcome, reason]);
  return sha256Hex(payload);
}
