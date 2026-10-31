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
const fallbackHash = await fallback.sha256Hex(message);
const first = await secure.computeAuditEntryHash('genesis', 1, 'Ingress', 'a:b', 'c', 'target', 1, 'AllowCleartext', 'ok');
const second = await secure.computeAuditEntryHash('genesis', 1, 'Ingress', 'a', 'b:c', 'target', 1, 'AllowCleartext', 'ok');
const results = [
  { name: 'webcrypto-sha256-known-answer', passed: secureHash === expected },
  { name: 'fallback-must-not-masquerade-as-sha256', passed: fallbackHash === expected,
    observed: 'Fallback returned a repeated 16-character pseudo-hash rather than SHA-256' },
  { name: 'frontend-audit-field-boundaries', passed: first !== second,
    observed: 'Distinct topic/source fields containing colons produced the same audit digest' },
];
await writeFile(new URL('frontend-crypto.json', import.meta.url), JSON.stringify(results, null, 2) + '\n');
for (const result of results) console.log(`${result.name}: ${result.passed ? 'PASS' : 'FAIL'}`);
process.exitCode = results.some(result => !result.passed) ? 1 : 0;
