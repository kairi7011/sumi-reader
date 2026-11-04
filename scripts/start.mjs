import { spawn } from 'node:child_process';
import { BINARY, ROOT, assertPackage, runtimeReady } from './electron-runtime.mjs';

try {
  await assertPackage();
  if (!await runtimeReady()) {
    throw new Error('Electron runtime is missing. Run npm run setup:runtime first; start never downloads code.');
  }
  const child = spawn(BINARY, [ROOT, ...process.argv.slice(2)], { stdio: 'inherit', env: process.env });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
  child.once('error', error => {
    console.error(`Unable to start Sumi: ${error.message}`);
    process.exitCode = 1;
  });
  child.once('exit', code => { process.exitCode = code ?? 1; });
} catch (error) {
  console.error(`Unable to start Sumi: ${error.message}`);
  process.exitCode = 1;
}
