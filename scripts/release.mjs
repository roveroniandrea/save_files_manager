#!/usr/bin/env node
// Release helper (see README "Releases" and docs/ARCHITECTURE.md "Build & packaging").
//
//   npm run release <X.Y.Z>        bump the version everywhere, move CHANGELOG "Unreleased"
//                                  into "## X.Y.Z — <date>", commit "Release vX.Y.Z" and tag vX.Y.Z
//                                  (nothing is pushed)
//   node scripts/release.mjs --check-tag vX.Y.Z   fail unless every version file and the
//                                                 changelog match the tag (used by release.yml)
//   node scripts/release.mjs --notes vX.Y.Z       print that version's changelog section
//   node scripts/release.mjs --self-test          test the text transformations (used by ci.yml)

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const FILES = {
  pkg: 'package.json',
  lock: 'package-lock.json',
  cargo: 'src-tauri/Cargo.toml',
  cargoLock: 'src-tauri/Cargo.lock',
  tauri: 'src-tauri/tauri.conf.json',
  changelog: 'CHANGELOG.md',
};
const CRATE = 'save-files-manager';
const SEMVER = /^\d+\.\d+\.\d+$/;

class ReleaseError extends Error {}
const fail = (msg) => {
  throw new ReleaseError(msg);
};

const read = (key) => readFileSync(join(root, FILES[key]), 'utf8');
const write = (key, text) => writeFileSync(join(root, FILES[key]), text);
const eol = (text) => (text.includes('\r\n') ? '\r\n' : '\n');
const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// ---- pure text transformations (covered by --self-test) ----

/** Replace the first top-level `"version": "…"` of a JSON file, keeping its formatting. */
export function setJsonVersion(text, version) {
  const re = /^(\s{2}"version":\s*")[^"]*(")/m;
  if (!re.test(text)) fail('no top-level "version" field found');
  return text.replace(re, `$1${version}$2`);
}

/** package-lock.json: the root version and the `packages[""]` version. */
export function setLockVersion(text, version) {
  const json = JSON.parse(text);
  json.version = version;
  if (json.packages?.['']) json.packages[''].version = version;
  const nl = eol(text);
  return JSON.stringify(json, null, 2).replace(/\n/g, nl) + nl;
}

export function getJsonVersion(text) {
  return JSON.parse(text).version;
}

/** Cargo.toml: `version` inside the `[package]` table. */
export function setCargoVersion(text, version) {
  const re = /(\[package\][^[]*?\nversion\s*=\s*")[^"]*(")/;
  if (!re.test(text)) fail('no [package] version found in Cargo.toml');
  return text.replace(re, `$1${version}$2`);
}

export function getCargoVersion(text) {
  const m = /\[package\][^[]*?\nversion\s*=\s*"([^"]*)"/.exec(text);
  return m ? m[1] : undefined;
}

/** Cargo.lock: the entry of this crate. */
export function setCargoLockVersion(text, version) {
  const re = new RegExp(`(name = "${escape(CRATE)}"\\r?\\nversion = ")[^"]*(")`);
  if (!re.test(text)) fail(`no ${CRATE} entry found in Cargo.lock`);
  return text.replace(re, `$1${version}$2`);
}

export function getCargoLockVersion(text) {
  const m = new RegExp(`name = "${escape(CRATE)}"\\r?\\nversion = "([^"]*)"`).exec(text);
  return m ? m[1] : undefined;
}

/** Split CHANGELOG.md into the text before the first `## ` heading and its sections. */
function sections(text) {
  const lines = text.split(/\r?\n/);
  const head = [];
  const out = [];
  for (const line of lines) {
    if (line.startsWith('## ')) out.push({ title: line.slice(3).trim(), body: [] });
    else if (out.length) out[out.length - 1].body.push(line);
    else head.push(line);
  }
  return { head, sections: out };
}

const trimLines = (lines) => {
  const l = [...lines];
  while (l.length && !l[0].trim()) l.shift();
  while (l.length && !l[l.length - 1].trim()) l.pop();
  return l;
};

const isVersionSection = (title, version) =>
  title === version || title.startsWith(`${version} `) || title === `v${version}` || title.startsWith(`v${version} `);

/** Move the `Unreleased` entries into a new `## <version> — <date>` section; leave `Unreleased` empty. */
export function moveUnreleased(text, version, date) {
  const nl = eol(text);
  const parsed = sections(text);
  const idx = parsed.sections.findIndex((s) => s.title.toLowerCase() === 'unreleased');
  if (idx < 0) fail('CHANGELOG.md has no "## Unreleased" section');
  if (parsed.sections.some((s) => isVersionSection(s.title, version))) fail(`CHANGELOG.md already has a ${version} section`);
  const entries = trimLines(parsed.sections[idx].body);
  if (!entries.length) fail('CHANGELOG.md "Unreleased" section is empty: nothing to release');
  const released = { title: `${version} — ${date}`, body: [...entries, ''] };
  parsed.sections.splice(idx, 1, { title: 'Unreleased', body: [''] }, released);
  const out = [...parsed.head];
  for (const s of parsed.sections) out.push(`## ${s.title}`, ...s.body);
  return out.join(nl).replace(/(\r?\n)*$/, nl);
}

/** Body of the `## <version> …` section (release notes). */
export function releaseNotes(text, version) {
  const s = sections(text).sections.find((x) => isVersionSection(x.title, version));
  if (!s) fail(`CHANGELOG.md has no section for ${version}`);
  const body = trimLines(s.body);
  if (!body.length) fail(`CHANGELOG.md section ${version} is empty`);
  return body.join('\n');
}

export function compareVersions(a, b) {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
}

function tagVersion(tag) {
  const v = tag.replace(/^refs\/tags\//, '').replace(/^v/, '');
  if (!SEMVER.test(v)) fail(`"${tag}" is not a vX.Y.Z tag`);
  return v;
}

// ---- commands ----

function currentVersions() {
  return {
    [FILES.pkg]: getJsonVersion(read('pkg')),
    [FILES.lock]: getJsonVersion(read('lock')),
    [FILES.cargo]: getCargoVersion(read('cargo')),
    [FILES.cargoLock]: getCargoLockVersion(read('cargoLock')),
    [FILES.tauri]: getJsonVersion(read('tauri')),
  };
}

function checkTag(tag) {
  const version = tagVersion(tag);
  const wrong = Object.entries(currentVersions()).filter(([, v]) => v !== version);
  if (wrong.length) {
    fail(`tag ${tag} does not match the app version:\n` + wrong.map(([f, v]) => `  ${f}: ${v}`).join('\n'));
  }
  releaseNotes(read('changelog'), version);
  console.log(`OK: ${tag} matches every version file and has a changelog section.`);
}

const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();

function localDate() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function release(version) {
  if (!SEMVER.test(version ?? '')) fail('usage: npm run release <X.Y.Z>');
  if (git('status', '--porcelain')) fail('the working tree is not clean: commit or stash first');
  const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
  if (branch !== 'main') fail(`releases are made from main (current branch: ${branch})`);
  const versions = currentVersions();
  const distinct = [...new Set(Object.values(versions))];
  if (distinct.length !== 1) fail('version files are out of sync:\n' + JSON.stringify(versions, null, 2));
  if (compareVersions(version, distinct[0]) <= 0) fail(`${version} must be greater than the current ${distinct[0]}`);
  if (git('tag', '--list', `v${version}`)) fail(`tag v${version} already exists`);

  // Compute everything first so a failure leaves no file half-updated.
  const changes = {
    changelog: moveUnreleased(read('changelog'), version, localDate()),
    pkg: setJsonVersion(read('pkg'), version),
    lock: setLockVersion(read('lock'), version),
    cargo: setCargoVersion(read('cargo'), version),
    cargoLock: setCargoLockVersion(read('cargoLock'), version),
    tauri: setJsonVersion(read('tauri'), version),
  };
  for (const [key, text] of Object.entries(changes)) write(key, text);
  git('add', ...Object.keys(changes).map((k) => FILES[k]));
  git('commit', '-m', `Release v${version}`);
  git('tag', '-a', `v${version}`, '-m', `Save Files Manager v${version}`);
  console.log(`Committed and tagged v${version} (nothing pushed).`);
  console.log(`Publish it with:  git push origin main && git push origin v${version}`);
  console.log(`Undo (before pushing):  git tag -d v${version} && git reset --hard HEAD~1`);
}

function selfTest() {
  const assert = (cond, msg) => {
    if (!cond) throw new Error(`self-test failed: ${msg}`);
  };
  const throws = (fn, msg) => {
    try {
      fn();
    } catch (e) {
      if (e instanceof ReleaseError) return;
      throw e;
    }
    throw new Error(`self-test failed (expected an error): ${msg}`);
  };

  const json = '{\n  "name": "x",\n  "version": "0.1.0",\n  "dependencies": { "a": { "version": "9.9.9" } }\n}\n';
  assert(getJsonVersion(setJsonVersion(json, '1.2.3')) === '1.2.3', 'json version');
  assert(setJsonVersion(json, '1.2.3').includes('"9.9.9"'), 'nested version untouched');

  const lock = JSON.stringify({ name: 'x', version: '0.1.0', packages: { '': { version: '0.1.0' }, 'node_modules/a': { version: '2.0.0' } } }, null, 2) + '\n';
  const lock2 = JSON.parse(setLockVersion(lock, '0.2.0'));
  assert(lock2.version === '0.2.0' && lock2.packages[''].version === '0.2.0', 'lock root');
  assert(lock2.packages['node_modules/a'].version === '2.0.0', 'lock dependency untouched');

  const toml = '[package]\r\nname = "save-files-manager"\r\nversion = "0.1.0"\r\n\r\n[dependencies]\r\nserde = { version = "1" }\r\n';
  assert(getCargoVersion(setCargoVersion(toml, '0.2.0')) === '0.2.0', 'cargo version');
  assert(setCargoVersion(toml, '0.2.0').includes('serde = { version = "1" }'), 'cargo deps untouched');

  const cl = '[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "save-files-manager"\nversion = "0.1.0"\n';
  const cl2 = setCargoLockVersion(cl, '0.3.0');
  assert(getCargoLockVersion(cl2) === '0.3.0' && cl2.includes('name = "other"\nversion = "0.1.0"'), 'cargo lock');

  const log = '# Changelog\n\nIntro.\n\n## Unreleased\n- New thing.\n- Fix.\n\n## 0.1.0 — 2026-01-01\n- First.\n';
  const moved = moveUnreleased(log, '0.2.0', '2026-10-20');
  assert(
    moved === '# Changelog\n\nIntro.\n\n## Unreleased\n\n## 0.2.0 — 2026-10-20\n- New thing.\n- Fix.\n\n## 0.1.0 — 2026-01-01\n- First.\n',
    'moveUnreleased output:\n' + moved,
  );
  assert(releaseNotes(moved, '0.2.0') === '- New thing.\n- Fix.', 'notes 0.2.0');
  assert(releaseNotes(moved, '0.1.0') === '- First.', 'notes 0.1.0');
  assert(moveUnreleased(log.replace(/\n/g, '\r\n'), '0.2.0', 'd').includes('\r\n## 0.2.0 — d\r\n'), 'CRLF kept');
  throws(() => moveUnreleased(moved, '0.3.0', 'd'), 'empty Unreleased');
  throws(() => moveUnreleased(log, '0.1.0', 'd'), 'duplicate version');
  throws(() => releaseNotes(log, '0.1.1'), 'missing section');
  assert(!isVersionSection('0.1.10 — x', '0.1.1'), 'prefix match');

  assert(compareVersions('0.10.0', '0.9.9') > 0 && compareVersions('1.0.0', '1.0.0') === 0, 'compare');
  assert(tagVersion('refs/tags/v1.2.3') === '1.2.3', 'tag ref');
  throws(() => tagVersion('v1.2'), 'bad tag');
  console.log('release.mjs self-test passed.');
}

try {
  const [cmd, arg] = process.argv.slice(2);
  if (cmd === '--self-test') selfTest();
  else if (cmd === '--check-tag') checkTag(arg ?? '');
  else if (cmd === '--notes') process.stdout.write(releaseNotes(read('changelog'), tagVersion(arg ?? '')) + '\n');
  else release(cmd);
} catch (e) {
  if (!(e instanceof ReleaseError)) throw e;
  console.error(`release: ${e.message}`);
  process.exit(1);
}
