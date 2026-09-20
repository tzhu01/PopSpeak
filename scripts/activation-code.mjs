#!/usr/bin/env node
// Operator-only tool. Do not ship signing keys or this operator workflow in the
// consumer portable package. The application receives only the raw public key.
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomUUID, sign, verify } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const repository = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function options(args) {
  const result = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!args[index]?.startsWith('--') || !args[index + 1] || args[index + 1].startsWith('--')) {
      throw new Error('Options must be --name value pairs.');
    }
    const key = args[index].slice(2);
    if (key in result) throw new Error(`Duplicate option: ${key}`);
    result[key] = args[index + 1];
  }
  return result;
}

function required(flags, key) {
  const value = flags[key];
  if (!value) throw new Error(`Missing --${key}`);
  return value;
}

function installationId(value) {
  if (!uuidPattern.test(value)) throw new Error('Installation ID must be the UUID shown in PopSpeak.');
  return value.toLowerCase();
}

function rawPublicKey(key) {
  const jwk = createPublicKey(key).export({ format: 'jwk' });
  if (jwk.kty !== 'OKP' || jwk.crv !== 'Ed25519') throw new Error('Only Ed25519 keys are supported.');
  return jwk.x;
}

function issue(privateKey, installId) {
  if (privateKey.asymmetricKeyType !== 'ed25519') throw new Error('Only Ed25519 signing keys are supported.');
  const claims = {
    v: 1,
    product: 'popspeak',
    install_id: installationId(installId),
    license_id: randomUUID(),
    issued_at: Math.floor(Date.now() / 1000),
    features: 'all_local',
  };
  const payload = Buffer.from(JSON.stringify(claims)).toString('base64url');
  const signed = `PS1.${payload}`;
  const signature = sign(null, Buffer.from(signed), privateKey).toString('base64url');
  return `${signed}.${signature}`;
}

function inspect(code, rawKey, expectedInstallId) {
  if (code.length > 4096) throw new Error('Code is too long.');
  const parts = code.trim().split('.');
  if (parts.length !== 3 || parts[0] !== 'PS1') throw new Error('Invalid code format.');
  const publicKey = createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: rawKey.trim() }, format: 'jwk' });
  if (!verify(null, Buffer.from(`PS1.${parts[1]}`), publicKey, Buffer.from(parts[2], 'base64url'))) {
    throw new Error('Signature verification failed.');
  }
  const claims = JSON.parse(Buffer.from(parts[1], 'base64url').toString('utf8'));
  if (claims.v !== 1 || claims.product !== 'popspeak' || claims.features !== 'all_local') {
    throw new Error('Code is not a supported PopSpeak license.');
  }
  if (expectedInstallId && claims.install_id !== installationId(expectedInstallId)) {
    throw new Error('Code belongs to another installation.');
  }
  return claims;
}

function keygen(flags) {
  const outputPath = required(flags, 'out-dir');
  if (!isAbsolute(outputPath)) throw new Error('--out-dir must be an absolute path outside the repository.');
  const resolved = resolve(outputPath);
  const relativePath = relative(repository, resolved);
  if (!relativePath || (!relativePath.startsWith(`..${sep}`) && relativePath !== '..' && !isAbsolute(relativePath))) {
    throw new Error('Signing keys must be stored outside the repository and all portable output directories.');
  }
  mkdirSync(resolved, { recursive: true });
  // Resolve symlinks/junctions as well: a path outside the repository may point
  // back into it. Refuse such paths before any private material is written.
  const actual = realpathSync(resolved);
  const actualRelative = relative(repository, actual);
  if (!actualRelative || (!actualRelative.startsWith(`..${sep}`) && actualRelative !== '..' && !isAbsolute(actualRelative))) {
    throw new Error('Signing-key target resolves inside the repository.');
  }
  const privateFile = resolve(actual, 'popspeak-signing-private.pem');
  const publicFile = resolve(actual, 'popspeak-public-key.txt');
  if (existsSync(privateFile) || existsSync(publicFile)) {
    throw new Error('A signing-key file already exists. Preserve the current key pair; choose a new empty directory only for an intentional key rotation.');
  }
  const { privateKey } = generateKeyPairSync('ed25519');
  // wx refuses to overwrite an existing key (rotating would invalidate codes).
  writeFileSync(privateFile, privateKey.export({ type: 'pkcs8', format: 'pem' }), { flag: 'wx', mode: 0o600 });
  const raw = rawPublicKey(privateKey);
  writeFileSync(publicFile, `${raw}\n`, { flag: 'wx', mode: 0o644 });
  process.stdout.write(`Private key saved (never share): ${privateFile}\nPublic key saved: ${publicFile}\nPOPSPEAK_ACTIVATION_PUBLIC_KEY=${raw}\n`);
}

function selfTest() {
  const { privateKey } = generateKeyPairSync('ed25519');
  const raw = rawPublicKey(privateKey);
  const installId = randomUUID();
  const code = issue(privateKey, installId);
  assert.equal(inspect(code, raw, installId).install_id, installId);
  assert.throws(() => inspect(code, raw, randomUUID()), /another installation/);
  const pieces = code.split('.');
  const claims = JSON.parse(Buffer.from(pieces[1], 'base64url').toString('utf8'));
  claims.install_id = randomUUID();
  pieces[1] = Buffer.from(JSON.stringify(claims)).toString('base64url');
  assert.throws(() => inspect(pieces.join('.'), raw), /Signature verification failed/);
  const other = generateKeyPairSync('ed25519');
  assert.throws(() => inspect(code, rawPublicKey(other.privateKey)), /Signature verification failed/);
  assert.throws(() => installationId('universal'), /UUID/);
  process.stdout.write('Activation self-test passed: sign/verify, wrong installation, changed payload, wrong key, malformed ID. No production keys or receipts were saved.\n');
}

try {
  const [command, ...args] = process.argv.slice(2);
  const flags = options(args);
  if (command === 'keygen') {
    keygen(flags);
  } else if (command === 'issue') {
    const key = createPrivateKey(readFileSync(required(flags, 'private-key')));
    process.stdout.write(`${issue(key, required(flags, 'install-id'))}\n`);
  } else if (command === 'inspect') {
    const key = readFileSync(required(flags, 'public-key'), 'utf8');
    process.stdout.write(`${JSON.stringify(inspect(required(flags, 'code'), key, flags['install-id']), null, 2)}\n`);
  } else if (command === 'self-test') {
    selfTest();
  } else {
    process.stdout.write(`PopSpeak activation operator tool\n\nnode scripts/activation-code.mjs keygen --out-dir D:\\popspeak-operator-private\nnode scripts/activation-code.mjs issue --private-key D:\\popspeak-operator-private\\popspeak-signing-private.pem --install-id <installation UUID>\nnode scripts/activation-code.mjs inspect --public-key D:\\popspeak-operator-private\\popspeak-public-key.txt --code <code>\nnode scripts/activation-code.mjs self-test\n\nPrivate keys never belong in the repository, AppData, or the portable package.\n`);
    if (command) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`Activation tool: ${error.message}\n`);
  process.exitCode = 1;
}
