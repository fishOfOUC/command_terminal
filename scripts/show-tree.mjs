import { readdirSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

// Prune generated/dependency directories before reading them; never follow links.
const excludedDirectories = new Set([
  '.git', 'node_modules', 'target', 'dist', 'build', 'out', 'coverage',
  'vendor', 'gen', '.qa', '.taste-skill', 'local-only', 'examples',
  '.cache', '.turbo', '.next', '.nuxt', '.output', '.svelte-kit',
  '.venv', 'venv', '__pycache__', '.pytest_cache', 'bin', 'obj'
]);
const excludedFile = name => /\.(?:log|db|db-shm|db-wal|pyc|pyo|tsbuildinfo|map)$/i.test(name)
  || name === '.DS_Store' || name === 'Thumbs.db'
  || (name.startsWith('.env') && name !== '.env.example');

const args = process.argv.slice(2);
let depth = 4;
let limit = 500;
try {
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--help' || args[i] === '-h') {
      console.log('Usage: npm run tree -- [--depth 1..10] [--limit 10..5000]\nShows the current directory; omits dependencies, build output, caches, local data and secrets.');
      process.exit(0);
    }
    const option = args[i];
    const value = Number(args[++i]);
    const range = option === '--depth' ? [1, 10] : option === '--limit' ? [10, 5000] : null;
    if (!range || !Number.isInteger(value) || value < range[0] || value > range[1]) {
      throw new Error(`Invalid ${option}; use --help for supported options.`);
    }
    if (option === '--depth') depth = value;
    else limit = value;
  }
} catch (error) {
  console.error(error.message);
  process.exit(2);
}

let shown = 0;
let capped = false;
let depthCapped = false;
const root = resolve('.');
// Include new source files, but honor the repository's .gitignore when Git is available.
const git = spawnSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', '.'], {
  cwd: root, encoding: 'utf8', windowsHide: true, maxBuffer: 16 * 1024 * 1024
});
let visiblePaths = null;
if (git.status === 0) {
  visiblePaths = new Set();
  for (const file of git.stdout.split('\0').filter(Boolean)) {
    const segments = file.replaceAll('\\', '/').split('/');
    for (let i = 1; i <= segments.length; i++) visiblePaths.add(segments.slice(0, i).join('/'));
  }
}
function walk(directory, prefix = '', level = 1, relative = '') {
  let entries;
  try {
    entries = readdirSync(directory, { withFileTypes: true })
      .filter(entry => !excludedDirectories.has(entry.name.toLowerCase()) && !excludedFile(entry.name)
        && (!visiblePaths || visiblePaths.has(relative + entry.name)))
      .sort((a, b) => Number(b.isDirectory()) - Number(a.isDirectory()) || a.name.localeCompare(b.name, 'en', { numeric: true }));
  } catch (error) {
    console.log(`${prefix}└── [unreadable: ${error.code}]`);
    return;
  }
  for (let i = 0; i < entries.length; i++) {
    if (shown >= limit) { capped = true; return; }
    const entry = entries[i];
    const last = i === entries.length - 1;
    const suffix = entry.isSymbolicLink() ? ' @' : entry.isDirectory() ? '/' : '';
    console.log(`${prefix}${last ? '└── ' : '├── '}${entry.name}${suffix}`);
    shown++;
    if (entry.isDirectory() && !entry.isSymbolicLink()) {
      if (level < depth) walk(join(directory, entry.name), `${prefix}${last ? '    ' : '│   '}`, level + 1, `${relative}${entry.name}/`);
      else depthCapped = true;
    }
    if (capped) return;
  }
}

console.log(`${basename(root)}/`);
walk(root);
console.log(`\n${shown} entries · dependencies, generated files and local data omitted.`);
if (depthCapped) console.log(`Depth limited to ${depth}; increase --depth to see deeper source directories.`);
if (capped) console.log(`Output limited to ${limit} entries; increase --limit to show more.`);
