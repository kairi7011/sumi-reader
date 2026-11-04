import { spawn } from 'node:child_process';
import path from 'node:path';
import { BINARY, ROOT, assertPackage, runtimeReady } from './electron-runtime.mjs';

await assertPackage();
if (!await runtimeReady()) throw new Error('Run npm run setup:runtime before the smoke test.');
const child = spawn(BINARY, [path.join(ROOT, 'tests/smoke.cjs')], { cwd: ROOT, stdio: 'inherit' });
child.on('error', error => { console.error(error); process.exitCode = 1; });
child.on('exit', code => { process.exitCode = code ?? 1; });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
