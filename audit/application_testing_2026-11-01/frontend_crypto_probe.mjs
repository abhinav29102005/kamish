// Isolated utility tests. Does not disable or modify browser security settings.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash, webcrypto } from 'node:crypto';
import vm from 'node:vm';
import ts from '../../packages/kamish-client/node_modules/typescript/lib/typescript.js';

const source = await readFile(new URL('../../frontend/src/utils/crypto.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;

function utility(crypto) {
  const context = vm.createContext({ exports: {}, crypto, TextEncoder });
  vm.runInContext(compiled, context, { timeout: 1000 });
  return context.exports;
}

const secure = utility(webcrypto);
const fallback = utility(undefined);
const message = 'synthetic application-test canary';
const expected = createHash('sha256').update(message).digest('hex');
const secureHash = await secure.sha256Hex(message);
let fallbackHash;
let rejectedUnavailableCrypto = false;
try { fallbackHash = await fallback.sha256Hex(message); }
catch (error) { rejectedUnavailableCrypto = /WebCrypto is unavailable/.test(String(error)); }
const first = await secure.computeAuditEntryHash('genesis', 1, 'Ingress', 'a:b', 'c', 'target', 1, 'AllowCleartext', 'ok');
const second = await secure.computeAuditEntryHash('genesis', 1, 'Ingress', 'a', 'b:c', 'target', 1, 'AllowCleartext', 'ok');
const results = [
  { name: 'webcrypto-sha256-known-answer', passed: secureHash === expected },
  { name: 'fallback-must-not-masquerade-as-sha256', passed: rejectedUnavailableCrypto || fallbackHash === expected,
    observed: rejectedUnavailableCrypto ? 'Unavailable WebCrypto was rejected explicitly' : 'Fallback output must equal real SHA-256' },
  { name: 'frontend-audit-field-boundaries', passed: first !== second,
    observed: 'Distinct topic/source fields containing colons produced the same audit digest' },
];
const destination = process.env.KAMISH_CRYPTO_OUTPUT
  ? new URL(process.env.KAMISH_CRYPTO_OUTPUT, `file://${process.cwd()}/`)
  : new URL('frontend-crypto.json', import.meta.url);
await writeFile(destination, JSON.stringify(results, null, 2) + '\n');
for (const result of results) console.log(`${result.name}: ${result.passed ? 'PASS' : 'FAIL'}`);
process.exitCode = results.some(result => !result.passed) ? 1 : 0;
