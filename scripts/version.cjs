// The reason why i use npm run bump <version> instead of bash script.
// Cross-platform compatibility (most critical reason: Windows native support)
// bash / sh scripts will fail on Windows by default, as Windows doesn't have sh interpreter by default.
// Also, we use JS/TS in frontend, so it's more convenient to use JS to bump version.

const fs = require('fs');
const path = require('path');

const version = process.argv[2];
if (!version) {
  console.error('Usage: node scripts/bump.cjs <new_version> (e.g. 0.1.3)');
  process.exit(1);
}

const cleanVer = version.replace(/^v/, '');
const rootDir = path.resolve(__dirname, '..');

// 1. package.json
const pkgPath = path.join(rootDir, 'package.json');
if (fs.existsSync(pkgPath)) {
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  pkg.version = cleanVer;
  fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
  console.log(`✓ Updated package.json -> ${cleanVer}`);
}

// 2. tauri.conf.json
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
if (fs.existsSync(tauriConfPath)) {
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
  tauriConf.version = cleanVer;
  fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
  console.log(`✓ Updated src-tauri/tauri.conf.json -> ${cleanVer}`);
}

// 3. Cargo.toml
const cargoPath = path.join(rootDir, 'src-tauri', 'Cargo.toml');
if (fs.existsSync(cargoPath)) {
  let cargoContent = fs.readFileSync(cargoPath, 'utf8');
  cargoContent = cargoContent.replace(/^version = ".*?"/m, `version = "${cleanVer}"`);
  fs.writeFileSync(cargoPath, cargoContent);
  console.log(`✓ Updated src-tauri/Cargo.toml -> ${cleanVer}`);
}
