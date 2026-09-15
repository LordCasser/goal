// Release signing uses one pinned certificate and one designated requirement.
// The private identity lives in CI secrets; only its public certificate is in Git.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const certificatePath = path.join(root, '.github/signing/macos-release.cer');
const identifier = 'dev.lordcasser.planner';
const secrets = new Set();
function run(command, args, { allowFailure = false } = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 120_000, killSignal: 'SIGKILL' });
  if (result.error || result.status !== 0) {
    if (allowFailure) return result;
    let detail = result.error?.message ?? result.stderr ?? '';
    for (const secret of secrets) if (secret) detail = detail.replaceAll(secret, '[redacted]');
    throw new Error(`${command} ${args[0] ?? ''} failed: ${detail.trim()}`);
  }
  return result;
}
function pinnedCertificate() {
  const bytes = fs.readFileSync(certificatePath);
  const certificate = new crypto.X509Certificate(bytes);
  return { bytes, fingerprint: certificate.fingerprint.replaceAll(':', ''), sha256: certificate.fingerprint256 };
}
export function designatedRequirement(fingerprint) {
  if (!/^[A-Fa-f0-9]{40}$/.test(fingerprint)) throw new Error('Invalid certificate fingerprint');
  return `designated => identifier "${identifier}" and certificate root = H"${fingerprint}"`;
}
function stateFile() {
  if (!process.env.GOAL_SIGNING_STATE_FILE) throw new Error('Run prepare and load its environment first');
  return process.env.GOAL_SIGNING_STATE_FILE;
}
function readState() { return JSON.parse(fs.readFileSync(stateFile(), 'utf8')); }
export function prepare() {
  const encoded = process.env.APPLE_CERTIFICATE;
  const password = process.env.APPLE_CERTIFICATE_PASSWORD;
  if (!encoded || !password) throw new Error('APPLE_CERTIFICATE and APPLE_CERTIFICATE_PASSWORD are required');
  if (!process.env.GITHUB_ENV) throw new Error('GITHUB_ENV must point to an environment output file');
  secrets.add(encoded); secrets.add(password);
  const pinned = pinnedCertificate();
  const directory = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'goal-signing-'));
  fs.chmodSync(directory, 0o700);
  const keychain = path.join(directory, 'release.keychain-db');
  const keychainPassword = crypto.randomBytes(32).toString('base64url');
  secrets.add(keychainPassword);
  const originalKeychains = [...run('security', ['list-keychains', '-d', 'user']).stdout.matchAll(/"([^"]+)"/g)].map(match => match[1]);
  const state = { directory, keychain, originalKeychains };
  const statePath = path.join(directory, 'state.json');
  fs.writeFileSync(statePath, JSON.stringify(state), { mode: 0o600 });
  // Persist cleanup location before the first keychain mutation.
  process.env.GOAL_SIGNING_STATE_FILE = statePath;
  fs.appendFileSync(process.env.GITHUB_ENV, `GOAL_SIGNING_STATE_FILE=${statePath}\n`);
  try {
    const p12 = path.join(directory, 'identity.p12');
    fs.writeFileSync(p12, Buffer.from(encoded, 'base64'), { mode: 0o600 });
    run('security', ['create-keychain', '-p', keychainPassword, keychain]);
    run('security', ['set-keychain-settings', '-lut', '21600', keychain]);
    run('security', ['unlock-keychain', '-p', keychainPassword, keychain]);
    run('security', ['list-keychains', '-d', 'user', '-s', keychain, ...originalKeychains]);
    run('security', ['import', p12, '-k', keychain, '-P', password, '-T', '/usr/bin/codesign']);
    fs.unlinkSync(p12);
    // Restrict the imported signing key to Apple signing tools, not all apps.
    run('security', ['set-key-partition-list', '-S', 'apple-tool:,apple:', '-s', '-k', keychainPassword, keychain]);
    const identities = run('security', ['find-identity', '-p', 'codesigning', keychain]).stdout;
    if (!identities.includes(pinned.fingerprint)) throw new Error('P12 identity does not match the pinned public certificate');
    // Explicit codesign identity + pinned DR do not require installing a trust
    // anchor. Keep user/admin trust settings untouched. Tauri's intermediate
    // bundle is signed below before it can become a release artifact.
    console.log(`Prepared pinned Goal signing identity ${pinned.fingerprint}`);
  } catch (error) { cleanup(); throw error; }
}
export function sign(appPath) {
  const state = readState();
  const pinned = pinnedCertificate();
  const app = path.resolve(appPath);
  if (!app.endsWith('.app') || !fs.existsSync(path.join(app, 'Contents/Info.plist'))) throw new Error('Expected a macOS .app bundle');
  const bundleId = run('/usr/libexec/PlistBuddy', ['-c', 'Print :CFBundleIdentifier', path.join(app, 'Contents/Info.plist')]).stdout.trim();
  if (bundleId !== identifier) throw new Error(`Bundle identifier must remain ${identifier}`);
  // Goal currently embeds no frameworks or helper apps. Refuse unexpected nested
  // code rather than accidentally leave it signed by a different identity.
  for (const name of ['Frameworks', 'PlugIns', 'XPCServices', 'Helpers']) {
    const directory = path.join(app, 'Contents', name);
    if (fs.existsSync(directory) && fs.readdirSync(directory).length) throw new Error(`Review nested-code signing before releasing Contents/${name}`);
  }
  run('codesign', ['--force', '--sign', pinned.fingerprint, '--keychain', state.keychain, '--timestamp=none', '--options', 'runtime', '--requirements', `=${designatedRequirement(pinned.fingerprint)}`, app]);
  verify(app);
}
export function verify(appPath) {
  const pinned = pinnedCertificate();
  run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  const requirement = designatedRequirement(pinned.fingerprint).replace(/^designated => /, '');
  run('codesign', ['--verify', '-R', `=${requirement}`, appPath]);
  const displayed = run('codesign', ['--display', '-r-', appPath]);
  const display = `${displayed.stdout}\n${displayed.stderr}`;
  const normalize = value => value.toLowerCase().replaceAll(/\s+/g, '');
  const actual = display.split('\n').find(line => line.startsWith('designated =>'));
  if (!actual || normalize(actual) !== normalize(designatedRequirement(pinned.fingerprint))) throw new Error('App designated requirement does not match the stable release identity');
  console.log(`Verified stable Goal identity: ${pinned.sha256}`);
}
export function packageApp(appPath, outputPath) {
  verify(appPath);
  const stage = fs.mkdtempSync(path.join(os.tmpdir(), 'goal-dmg-'));
  try {
    run('ditto', [appPath, path.join(stage, path.basename(appPath))]);
    fs.symlinkSync('/Applications', path.join(stage, 'Applications'));
    fs.mkdirSync(path.dirname(path.resolve(outputPath)), { recursive: true });
    run('hdiutil', ['create', '-volname', 'Goal', '-srcfolder', stage, '-format', 'UDZO', '-ov', outputPath]);
    run('hdiutil', ['verify', outputPath]);
  } finally { fs.rmSync(stage, { recursive: true, force: true }); }
}
export function cleanup() {
  const file = process.env.GOAL_SIGNING_STATE_FILE;
  if (!file || !fs.existsSync(file)) return;
  const state = readState();
  run('security', ['list-keychains', '-d', 'user', '-s', ...state.originalKeychains]);
  run('security', ['delete-keychain', state.keychain], { allowFailure: true });
  fs.rmSync(state.directory, { recursive: true, force: true });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.platform !== 'darwin') throw new Error('macOS signing requires a macOS host');
    const [command, ...args] = process.argv.slice(2);
    if (command === 'prepare') prepare();
    else if (command === 'sign' && args.length === 1) sign(args[0]);
    else if (command === 'verify' && args.length === 1) verify(args[0]);
    else if (command === 'package' && args.length === 2) packageApp(...args);
    else if (command === 'cleanup') cleanup();
    else throw new Error('Usage: macos-signing.mjs prepare|sign APP|verify APP|package APP DMG|cleanup');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
