#!/usr/bin/env node

// Fix package.json after wasm-pack build
// wasm-pack overwrites package.json, so we need to fix the package name

const fs = require('fs');
const path = require('path');

const packagePath = path.join(__dirname, '..', 'pkg', 'package.json');

if (fs.existsSync(packagePath)) {
  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  
  // Update package name to scoped name
  packageJson.name = '@officialunofficial/trek';
  
  // Write back
  fs.writeFileSync(packagePath, JSON.stringify(packageJson, null, 2) + '\n');
  
  console.log('✅ Fixed package.json: name updated to @officialunofficial/trek');
} else {
  console.error('❌ Error: pkg/package.json not found');
  process.exit(1);
}