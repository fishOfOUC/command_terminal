import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const version=JSON.parse(read('package.json')).version;
const versions=[JSON.parse(read('package-lock.json')).version,JSON.parse(read('package-lock.json')).packages[''].version,
  JSON.parse(read('src-tauri/tauri.conf.json')).version,read('Cargo.toml').match(/\[workspace.package\]\s*version\s*=\s*"([^"]+)"/)[1]];
if(versions.some(v=>v!==version))throw Error(`Version mismatch: ${[version,...versions].join(', ')}`);
if(!/^\d+\.\d+\.\d+$/.test(version))throw Error('Portable stable releases require a major.minor.patch version');
if(process.env.GITHUB_REF_TYPE==='tag' && process.env.GITHUB_REF_NAME!==`v${version}`)throw Error('Release tag must match package version');
console.log(`Release version: ${version}`);
