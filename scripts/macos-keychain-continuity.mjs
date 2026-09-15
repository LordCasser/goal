// Release smoke test for the Keychain designated-requirement contract.
//
// The signing keychain is prepared by macos-signing.mjs. This script only
// reads that state, signs synthetic test binaries, and creates/deletes its own
// temporary test keychain. It never opens the user's default keychain.
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { designatedRequirement } from './macos-signing.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const certificatePath = path.join(root, '.github/signing/macos-release.cer');

function run(command, args, { redact = [] } = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    timeout: 120_000,
  });
  if (result.error || result.status !== 0) {
    const safeArgs = args.map((arg) => redact.includes(arg) ? '[redacted]' : arg);
    let detail = `${result.stderr ?? ''}`.trim();
    for (const value of redact) detail = detail.replaceAll(value, '[redacted]');
    throw new Error(`${command} ${safeArgs.join(' ')} failed (${result.error?.code ?? result.status ?? 'unknown'})${detail ? `: ${detail}` : ''}`);
  }
  return result;
}

function readSigningState() {
  const statePath = process.env.GOAL_SIGNING_STATE_FILE;
  if (!statePath) throw new Error('GOAL_SIGNING_STATE_FILE is required; run macos-signing.mjs prepare first');
  let state;
  try {
    state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  } catch {
    throw new Error('GOAL_SIGNING_STATE_FILE is not readable');
  }
  if (typeof state.keychain !== 'string' || !fs.existsSync(state.keychain)) {
    throw new Error('Prepared signing keychain is missing');
  }
  return state;
}

function certificateFingerprint() {
  let certificate;
  try {
    certificate = new crypto.X509Certificate(fs.readFileSync(certificatePath));
  } catch {
    throw new Error('Pinned macOS release certificate is not readable');
  }
  const fingerprint = certificate.fingerprint.replaceAll(':', '');
  if (!/^[A-Fa-f0-9]{40}$/.test(fingerprint)) {
    throw new Error('Pinned macOS release certificate has an invalid SHA-1 fingerprint');
  }
  return fingerprint;
}

function sourceFor(marker) {
  return `#include <Security/Security.h>
#include <stdio.h>
#include <string.h>

static const char *const build_marker = "${marker}";
static const char *const service = "dev.lordcasser.planner.keychain-continuity-test";
static const char *const account = "synthetic-continuity-account";
static const char *const password = "synthetic-continuity-value";

static int report_status(const char *operation, OSStatus status, int exit_code) {
    fprintf(stderr, "keychain continuity %s OSStatus=%d\\n", operation, (int)status);
    return exit_code;
}

static int open_keychain(const char *path, SecKeychainRef *keychain) {
    if (build_marker[0] == '\\0') return report_status("build-marker", errSecParam, 90);
    return (int)SecKeychainOpen(path, keychain);
}

static int add_item(const char *path) {
    SecKeychainRef keychain = NULL;
    OSStatus status = (OSStatus)open_keychain(path, &keychain);
    if (status != errSecSuccess) return report_status("open-add-keychain", status, 2);
    SecKeychainItemRef item = NULL;
    status = SecKeychainAddGenericPassword(
        keychain,
        (UInt32)strlen(service), service,
        (UInt32)strlen(account), account,
        (UInt32)strlen(password), password,
        &item
    );
    if (status == errSecSuccess && item != NULL) {
        // SecKeychainAddGenericPassword's default ACL trusts only the
        // creating application. Set it explicitly so this test exercises
        // the same generic-password ACL boundary as the app's keyring.
        SecAccessRef access = NULL;
        status = SecAccessCreate(CFSTR("Goal Keychain continuity fixture"), NULL, &access);
        if (status == errSecSuccess) status = SecKeychainItemSetAccess(item, access);
        if (access != NULL) CFRelease(access);
        CFRelease(item);
    }
    CFRelease(keychain);
    return status == errSecSuccess ? 0 : report_status("add", status, 1);
}

static int find_item(const char *path, int should_find) {
    SecKeychainRef keychain = NULL;
    OSStatus status = (OSStatus)open_keychain(path, &keychain);
    if (status != errSecSuccess) return report_status("open-find-keychain", status, 2);
    // A successful continuity check must not be satisfied by a UI prompt.
    status = SecKeychainSetUserInteractionAllowed(false);
    if (status != errSecSuccess) {
        CFRelease(keychain);
        return report_status("disable-keychain-ui", status, 2);
    }
    UInt32 password_length = 0;
    void *password_data = NULL;
    status = SecKeychainFindGenericPassword(
        keychain,
        (UInt32)strlen(service), service,
        (UInt32)strlen(account), account,
        &password_length, &password_data, NULL
    );
    if (password_data != NULL) SecKeychainItemFreeContent(NULL, password_data);
    CFRelease(keychain);
    if (should_find) return status == errSecSuccess ? 0 : report_status("find", status, 1);
    return status == errSecSuccess ? report_status("unexpected-find-success", status, 1) : 0;
}

int main(int argc, char **argv) {
    if (argc != 3) return 90;
    if (strcmp(argv[1], "add") == 0) return add_item(argv[2]);
    if (strcmp(argv[1], "find") == 0) return find_item(argv[2], 1);
    if (strcmp(argv[1], "deny") == 0) return find_item(argv[2], 0);
    return 90;
}
`;
}

function compileBinary(source, output) {
  run('clang', [source, '-framework', 'Security', '-framework', 'CoreFoundation', '-O0', '-o', output]);
}

function signBinary(binary, identity, keychain, identifier, requirement) {
  run('codesign', [
    '--force',
    '--sign', identity,
    '--identifier', identifier,
    '--keychain', keychain,
    '--timestamp=none',
    '--requirements', `=${requirement}`,
    binary,
  ]);
  run('codesign', ['--verify', '--strict', binary]);
}

function signAdHoc(binary, identifier) {
  run('codesign', ['--force', '--sign', '-', '--identifier', identifier, '--timestamp=none', binary]);
  run('codesign', ['--verify', '--strict', binary]);
}

function cdHash(binary) {
  const result = run('codesign', ['--display', '--verbose=4', binary]);
  const match = `${result.stdout}\n${result.stderr}`.match(/(?:^|\n)CDHash=([A-Fa-f0-9]+)/);
  if (!match) throw new Error('Signed continuity fixture has no CDHash');
  return match[1].toLowerCase();
}

function makeKeychain(directory) {
  const keychain = path.join(directory, 'continuity-test.keychain-db');
  const password = crypto.randomBytes(24).toString('base64url');
  run('security', ['create-keychain', '-p', password, keychain], { redact: [password] });
  run('security', ['set-keychain-settings', '-lut', '21600', keychain]);
  run('security', ['unlock-keychain', '-p', password, keychain], { redact: [password] });
  return keychain;
}

function main() {
  if (process.platform !== 'darwin') throw new Error('macOS Keychain continuity requires a macOS host');
  const signingState = readSigningState();
  const fingerprint = certificateFingerprint();
  const stableIdentifier = 'dev.lordcasser.planner';
  const stableRequirement = designatedRequirement(fingerprint);
  if (!stableRequirement.includes(`identifier "${stableIdentifier}"`)) {
    throw new Error('Stable signing requirement identifier does not match the continuity fixture');
  }
  const wrongIdentifierRequirement = stableRequirement.replace(
    'dev.lordcasser.planner',
    'dev.lordcasser.planner.keychain-negative',
  );
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'goal-keychain-continuity-'));
  fs.chmodSync(directory, 0o700);
  let keychain;
  try {
    keychain = makeKeychain(directory);
    const sourceV1 = path.join(directory, 'continuity-v1.c');
    const sourceV2 = path.join(directory, 'continuity-v2.c');
    const v1 = path.join(directory, 'continuity-v1');
    const v2 = path.join(directory, 'continuity-v2');
    const wrongIdentifier = path.join(directory, 'continuity-wrong-identifier');
    const wrongSignature = path.join(directory, 'continuity-wrong-signature');
    fs.writeFileSync(sourceV1, sourceFor('goal-keychain-continuity-v1'));
    fs.writeFileSync(sourceV2, sourceFor('goal-keychain-continuity-v2'));
    compileBinary(sourceV1, v1);
    compileBinary(sourceV2, v2);
    fs.copyFileSync(v2, wrongIdentifier);
    fs.copyFileSync(v2, wrongSignature);

    signBinary(v1, fingerprint, signingState.keychain, stableIdentifier, stableRequirement);
    signBinary(v2, fingerprint, signingState.keychain, stableIdentifier, stableRequirement);
    if (cdHash(v1) === cdHash(v2)) throw new Error('Continuity fixtures unexpectedly share a CDHash');

    run(v1, ['add', keychain]);
    run(v2, ['find', keychain]);

    signBinary(wrongIdentifier, fingerprint, signingState.keychain, 'dev.lordcasser.planner.keychain-negative', wrongIdentifierRequirement);
    run(wrongIdentifier, ['deny', keychain]);

    signAdHoc(wrongSignature, stableIdentifier);
    run(wrongSignature, ['deny', keychain]);
    console.log('macOS Keychain continuity passed: stable DR allowed access; wrong identifier and signature were denied.');
  } finally {
    if (keychain) run('security', ['delete-keychain', keychain]);
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : 'macOS Keychain continuity failed');
  process.exitCode = 1;
}
