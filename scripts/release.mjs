#!/usr/bin/env node
/**
 * Поднимает версию во всех местах разом и заводит раздел в CHANGELOG.md.
 *
 *   npm run release -- 0.2.0
 *
 * Версия живёт в package.json (из него её берёт tauri.conf.json) и в
 * src-tauri/Cargo.toml; package-lock.json и Cargo.lock правятся следом.
 * Коммит и тег скрипт не делает — команды для них он печатает в конце:
 * тег запускает сборку релиза на GitHub, и ставить его лучше осознанно.
 */
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2]?.replace(/^v/, '');
// MSI принимает только «число.число.число», поэтому суффиксов вроде -beta нет.
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error('Укажи версию вида 1.2.3: npm run release -- 1.2.3');
  process.exit(1);
}

function edit(path, change) {
  const before = readFileSync(path, 'utf8');
  const after = change(before);
  if (after === before) throw new Error(`${path}: версия не нашлась`);
  writeFileSync(path, after);
}

const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
const previous = pkg.version;
if (previous === version) {
  console.error(`Версия уже ${version}.`);
  process.exit(1);
}

edit('package.json', (text) => text.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
edit('package-lock.json', (text) => {
  const lock = JSON.parse(text);
  lock.version = version;
  if (lock.packages?.['']) lock.packages[''].version = version;
  return `${JSON.stringify(lock, null, 2)}\n`;
});
edit('src-tauri/Cargo.toml', (text) => text.replace(/^version = "[^"]+"/m, `version = "${version}"`));
// Cargo.lock догоняет сам, без сети.
execFileSync('cargo', ['metadata', '--offline', '--format-version', '1', '--manifest-path', 'src-tauri/Cargo.toml'], {
  stdio: 'ignore',
});

const today = new Date().toISOString().slice(0, 10);
edit('CHANGELOG.md', (text) =>
  text.replace(/^(## )/m, `## [${version}] — ${today}\n\n- \n\n$1`),
);

console.log(`Версия ${previous} → ${version}. Дальше:
  1. Допиши изменения в CHANGELOG.md (раздел ${version}) — это станет описанием релиза.
  2. git commit -am "Ringloft ${version}"
  3. git tag v${version} && git push && git push origin v${version}
  4. Когда сборка на GitHub закончится — проверь черновик релиза и опубликуй.`);
