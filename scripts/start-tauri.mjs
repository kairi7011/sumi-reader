import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const binary = fileURLToPath(new URL('../src-tauri/target/aarch64-apple-darwin/debug/sumi', import.meta.url));
const processHandle = spawn(binary, process.argv.slice(2), { stdio: 'inherit' });
processHandle.on('error', error => { console.error(error.message); process.exitCode = 1; });
processHandle.on('exit', code => { process.exitCode = code || 0; });
