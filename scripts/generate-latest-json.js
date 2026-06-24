#!/usr/bin/env node

import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';

const [assetsDir, version, tagName, repository] = process.argv.slice(2);

if (!assetsDir || !version || !tagName || !repository) {
  console.error(
    'Usage: node scripts/generate-latest-json.js <assets-dir> <version> <tag-name> <owner/repo>'
  );
  process.exit(1);
}

const requiredPlatforms = (process.env.REQUIRED_UPDATER_PLATFORMS ??
  'windows-x86_64,darwin-aarch64,darwin-x86_64,linux-x86_64')
  .split(',')
  .map((value) => value.trim())
  .filter(Boolean);

function walkFiles(dir) {
  const entries = readdirSync(dir).flatMap((entry) => {
    const fullPath = join(dir, entry);
    return statSync(fullPath).isDirectory() ? walkFiles(fullPath) : [fullPath];
  });
  return entries;
}

function inferPlatform(assetName) {
  const name = assetName.toLowerCase();

  if (name.includes('windows') || name.includes('nsis') || name.endsWith('.msi') || name.endsWith('.exe')) {
    return 'windows-x86_64';
  }

  if (name.includes('darwin') || name.includes('macos') || name.endsWith('.dmg')) {
    if (name.includes('aarch64') || name.includes('arm64')) {
      return 'darwin-aarch64';
    }
    return 'darwin-x86_64';
  }

  if (name.includes('linux') || name.includes('appimage') || name.endsWith('.deb') || name.endsWith('.rpm')) {
    return 'linux-x86_64';
  }

  return null;
}

function scoreAsset(assetName) {
  const name = assetName.toLowerCase();
  let score = 0;

  if (name.endsWith('.tar.gz')) score += 80;
  if (name.endsWith('.zip')) score += 70;
  if (name.includes('nsis')) score += 20;
  if (name.includes('appimage')) score += 15;
  if (name.endsWith('.dmg')) score += 10;
  if (name.endsWith('.msi')) score += 10;
  if (name.endsWith('.deb')) score += 5;

  return score;
}

function releaseDownloadUrl(assetName) {
  return `https://github.com/${repository}/releases/download/${tagName}/${encodeURIComponent(assetName)}`;
}

const signatureFiles = walkFiles(assetsDir).filter((file) => file.endsWith('.sig'));
const candidates = [];

for (const signaturePath of signatureFiles) {
  const assetName = basename(signaturePath, '.sig');
  const assetPath = join(dirname(signaturePath), assetName);

  if (!existsSync(assetPath)) {
    console.warn(`Skipping ${basename(signaturePath)} because ${assetName} is missing.`);
    continue;
  }

  const platform = inferPlatform(assetName);
  if (!platform) {
    console.warn(`Skipping ${assetName}; could not infer updater platform.`);
    continue;
  }

  candidates.push({
    platform,
    assetName,
    signature: readFileSync(signaturePath, 'utf8').trim(),
    score: scoreAsset(assetName),
  });
}

const platforms = {};
for (const candidate of candidates) {
  const existing = platforms[candidate.platform];
  if (!existing || candidate.score > existing.score) {
    platforms[candidate.platform] = candidate;
  }
}

const missing = requiredPlatforms.filter((platform) => !platforms[platform]);
if (missing.length > 0) {
  console.error(`Missing updater signatures for: ${missing.join(', ')}`);
  console.error('Signed candidates found:');
  for (const candidate of candidates) {
    console.error(`  ${candidate.platform}: ${candidate.assetName}`);
  }
  process.exit(1);
}

const manifest = {
  version,
  notes: `See the release notes for ${tagName}.`,
  pub_date: new Date().toISOString(),
  platforms: Object.fromEntries(
    Object.entries(platforms).map(([platform, candidate]) => [
      platform,
      {
        signature: candidate.signature,
        url: releaseDownloadUrl(candidate.assetName),
      },
    ])
  ),
};

mkdirSync(assetsDir, { recursive: true });
writeFileSync(join(assetsDir, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`);

console.log('Generated updater manifest for platforms:');
for (const [platform, candidate] of Object.entries(platforms)) {
  console.log(`  ${platform}: ${candidate.assetName}`);
}
