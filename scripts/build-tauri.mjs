import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workspaceRust = path.resolve(root, '../toolchains/rust-1.99.1/bin');
const rustBin = process.env.SUMI_RUST_BIN || workspaceRust;
const localToolchain = await fs.access(path.join(rustBin, 'cargo')).then(() => true, () => false);
if (process.env.RUSTC_WRAPPER || process.env.RUSTC_WORKSPACE_WRAPPER) throw new Error('Remove unreviewed Rust compiler wrappers before building.');
const environment = { ...process.env, MACOSX_DEPLOYMENT_TARGET: '13.0', TEST_SWIFT_RS: 'false' };
if (localToolchain) {
  environment.PATH = `${rustBin}${path.delimiter}${environment.PATH}`;
  environment.CARGO_HOME ||= path.resolve(root, '../cargo');
  environment.RUSTC = path.join(rustBin, 'rustc');
}
const cargo = localToolchain ? path.join(rustBin, 'cargo') : 'cargo';
const debug = process.argv.includes('--debug');
const test = process.argv.includes('--test');
const command = [test ? 'test' : 'build', '--locked', '--offline', '--manifest-path', path.join(root, 'src-tauri/Cargo.toml'), '--target', 'aarch64-apple-darwin'];
if (!debug && !test) command.push('--release');
execFileSync(cargo, command, { cwd: root, env: environment, stdio: 'inherit' });
