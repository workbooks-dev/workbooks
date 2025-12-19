#!/usr/bin/env node

/**
 * Update version across all project files
 * Usage: npm run version <new-version>
 * Example: npm run version 0.2.0
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Read current version from package.json
const packageJsonPath = path.join(__dirname, '..', 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const currentVersion = packageJson.version;

// Determine new version
let newVersion = process.argv[2];

if (!newVersion) {
  // Auto-increment patch version (x.y.Z)
  const [major, minor, patch] = currentVersion.split('.').map(n => parseInt(n.split('-')[0]));
  newVersion = `${major}.${minor}.${patch + 1}`;
  console.log(`Auto-incrementing patch version: ${currentVersion} → ${newVersion}\n`);
} else {
  // Validate semver format
  if (!/^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$/.test(newVersion)) {
    console.error('Error: Invalid version format. Use semver (e.g., 0.2.0 or 0.2.0-beta.1)');
    process.exit(1);
  }
  console.log(`Updating project version: ${currentVersion} → ${newVersion}\n`);
}

console.log(`Setting version to ${newVersion}...\n`);

// 1. Update package.json
packageJson.version = newVersion;
fs.writeFileSync(packageJsonPath, JSON.stringify(packageJson, null, 2) + '\n');
console.log('✓ Updated package.json');

// 2. Update Cargo.toml
const cargoTomlPath = path.join(__dirname, '..', 'src-tauri', 'Cargo.toml');
let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
cargoToml = cargoToml.replace(/^version = ".*"$/m, `version = "${newVersion}"`);
fs.writeFileSync(cargoTomlPath, cargoToml);
console.log('✓ Updated src-tauri/Cargo.toml');

// 3. Update tauri.conf.json
const tauriConfPath = path.join(__dirname, '..', 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
console.log('✓ Updated src-tauri/tauri.conf.json');

console.log(`\n✅ All version numbers updated to ${newVersion}`);
console.log('\nNext steps:');
console.log('1. Review changes: git diff');
console.log('2. Commit: git add -A && git commit -m "Bump version to ' + newVersion + '"');
console.log('3. Tag: git tag v' + newVersion);
console.log('4. Push: git push && git push --tags');
