import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
if (process.platform !== 'darwin' || process.arch !== 'arm64') throw new Error('Build Sumi on an Apple Silicon Mac.');
if (process.env.RUSTC_WRAPPER || process.env.RUSTC_WORKSPACE_WRAPPER) throw new Error('Remove unreviewed Rust compiler wrappers before building.');
const exists = async target => fs.access(target).then(() => true, () => false);
const local = await fs.readFile(path.join(root, '.native-toolchain.json'), 'utf8').then(JSON.parse, () => ({}));
let rustBin = process.env.SUMI_RUST_BIN || local.rustBin;
let cargoHome = process.env.CARGO_HOME || local.cargoHome;
if (!rustBin) {
  const candidates = [path.resolve(root, '../toolchains/rust-1.99.1/bin')];
  const workspaces = path.join(os.homedir(), 'Documents/Codex');
  for (const entry of await fs.readdir(workspaces, { withFileTypes: true }).catch(() => [])) {
    if (!entry.isDirectory()) continue;
    const directory = path.join(workspaces, entry.name);
    candidates.push(path.join(directory, 'work/toolchains/rust-1.99.1/bin'));
    for (const task of await fs.readdir(directory, { withFileTypes: true }).catch(() => [])) {
      if (task.isDirectory()) candidates.push(path.join(directory, task.name, 'work/toolchains/rust-1.99.1/bin'));
    }
  }
  for (const candidate of candidates) {
    if (await exists(path.join(candidate, 'cargo'))) { rustBin = candidate; cargoHome ||= path.resolve(candidate, '../../../cargo'); break; }
  }
}
const environment = { ...process.env, MACOSX_DEPLOYMENT_TARGET: '13.0' };
if (rustBin) { environment.PATH = `${rustBin}${path.delimiter}${environment.PATH}`; environment.RUSTC = path.join(rustBin, 'rustc'); }
if (cargoHome) environment.CARGO_HOME = cargoHome;
const cargo = rustBin ? path.join(rustBin, 'cargo') : 'cargo';
const rustc = rustBin ? path.join(rustBin, 'rustc') : 'rustc';
if (!execFileSync(rustc, ['--version'], { env: environment, encoding: 'utf8' }).startsWith('rustc 1.99.1 ')) throw new Error('The reviewed build uses Rust 1.99.1. Set SUMI_RUST_BIN and CARGO_HOME to its verified local installation.');
const test = process.argv.includes('--test');
const check = process.argv.includes('--check');
const command = [test ? 'test' : check ? 'check' : 'build', '--locked', '--offline', '--manifest-path', path.join(root, 'src-native/Cargo.toml'), '--target', 'aarch64-apple-darwin'];
if (process.argv.includes('--lib')) command.push('--lib');
if (!test && !check && !process.argv.includes('--debug')) command.push('--release');
execFileSync(cargo, command, { cwd: root, env: environment, stdio: 'inherit' });
