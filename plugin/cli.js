#!/usr/bin/env node
/* figleaf exporter on the command line: the plugin's Rust, from a snapshot.
 *
 *   node plugin/cli.js request <node-id> [part]   code for one Figma API call
 *   node plugin/cli.js join <part.json>... > snapshot.json
 *   node plugin/cli.js export <snapshot.json> > component.rs
 *
 * `request` prints plugin/snapshot.js plus the one line that runs it; an
 * agent sends that through Figma's MCP `use_figma` tool and saves each
 * returned part. `join` checks the parts belong together and writes the
 * snapshot. `export` replays the snapshot into the same exporter the plugin
 * runs (exporter.js, variables.js, assets.js) and prints its Rust, or its
 * errors with layer paths and exit status 1.
 *
 * It reads only the files it is given and writes only stdout/stderr. No
 * dependencies; Node 18 or later. */
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const here = __dirname;
const read = name => fs.readFileSync(path.join(here, name), 'utf8');
const { exportHierarchy } = require('./exporter.js');
// variables.js and assets.js expect their neighbours in scope, as in the
// plugin bundle; load them the way exporter.test.js does.
const resolver = { exportHierarchy, module: { exports: {} } };
vm.runInNewContext(read('variables.js'), resolver);
const { exportWithVariables } = resolver.module.exports;
const assets = { exportWithVariables, module: { exports: {} } };
vm.runInNewContext(read('assets.js'), assets);
const { exportWithAssets } = assets.module.exports;
const { figleafSnapshotHash } = require('./snapshot.js');

// A snapshot node back into the shape the exporter reads off a live one.
function hydrate(n) {
  if (n === null || typeof n !== 'object') return n;
  if (Array.isArray(n)) return n.map(hydrate);
  if (n.$mixed === true && Object.keys(n).length === 1) return Symbol('mixed');
  const out = {};
  for (const [k, v] of Object.entries(n)) out[k] = hydrate(v);
  if (Array.isArray(out.styledSegments)) {
    const segments = out.styledSegments;
    out.getStyledTextSegments = () => segments;
  }
  return out;
}

// Rust for a snapshot object: `{code, errors}`, as the plugin reports it. A
// bare node (the older `*-reference.json` fixtures) is accepted as a
// snapshot with no variables or pattern sources.
async function exportSnapshot(snapshot) {
  const envelope = snapshot && snapshot.figleafSnapshot !== undefined
    ? snapshot
    : { figleafSnapshot: 1, colorProfile: 'SRGB', root: snapshot, variables: {}, sources: {} };
  if (envelope.figleafSnapshot !== 1) {
    return { code: null, errors: [{ path: 'Snapshot', message: `Unsupported snapshot version ${envelope.figleafSnapshot}; this CLI reads version 1.` }] };
  }
  const root = hydrate(envelope.root);
  const sources = new Map(Object.entries(envelope.sources || {}).map(([id, n]) => [id, n && hydrate(n)]));
  const recorded = envelope.variables || {};
  const variables = {
    async getVariableByIdAsync(id) {
      const v = recorded[id];
      if (!v) return null;
      return {
        name: v.name,
        variableCollectionId: v.variableCollectionId,
        // The value Figma resolved for this consumer when the snapshot was
        // taken; a node it was not taken at has none, and the exporter says so.
        resolveForConsumer: node => hydrate(v.resolved[node.id]) || { resolvedType: 'UNRESOLVED', value: undefined },
      };
    },
  };
  const api = {
    variables,
    async getNodeByIdAsync(id) { return sources.get(id) || null; },
    getImageByHash(hash) {
      return {
        async getSizeAsync() { throw Error(`Image ${hash}: the CLI cannot read image fills; export this hierarchy with the plugin.`); },
      };
    },
  };
  const decode = () => { throw Error('The CLI cannot decode images; export this hierarchy with the plugin.'); };
  return exportWithAssets([root], envelope.colorProfile || 'SRGB', api, decode);
}

// The snapshot text from its parts, or an error naming what is wrong.
function joinParts(parts) {
  if (!parts.length) throw Error('No parts given.');
  const first = parts[0];
  const want = first.parts;
  for (const p of parts) {
    if (p.figleafSnapshotPart === undefined || typeof p.data !== 'string') throw Error('Not a snapshot part: each needs figleafSnapshotPart and data.');
    if (p.parts !== want || p.length !== first.length || p.hash !== first.hash) {
      throw Error(`Part ${p.figleafSnapshotPart} is from a different snapshot (hash ${p.hash}, expected ${first.hash}). Take every part again.`);
    }
  }
  const byIndex = new Map(parts.map(p => [p.figleafSnapshotPart, p]));
  const missing = [];
  for (let i = 0; i < want; i++) if (!byIndex.has(i)) missing.push(i);
  if (missing.length) throw Error(`Missing part${missing.length > 1 ? 's' : ''} ${missing.join(', ')} of ${want}.`);
  const text = Array.from({ length: want }, (_, i) => byIndex.get(i).data).join('');
  if (text.length !== first.length || figleafSnapshotHash(text) !== first.hash) throw Error('The joined text does not match its hash; take the snapshot again.');
  return text;
}

// The code an agent sends through `use_figma` for one part.
function requestCode(nodeId, part = 0) {
  if (!/^(I?\d+:\d+)(;\d+:\d+)*$/.test(nodeId)) throw Error(`"${nodeId}" is not a Figma node id like 422:319.`);
  return `${read('snapshot.js')}\nreturn await figleafSnapshotPart(${JSON.stringify(nodeId)}, ${part});\n`;
}

const usage = `usage:
  node plugin/cli.js request <node-id> [part]      print the code for one use_figma call
  node plugin/cli.js join <part.json>...           join saved parts into one snapshot (stdout)
  node plugin/cli.js export <snapshot.json>        print the Rust for a snapshot (stdout)`;

async function main(argv) {
  const [command, ...args] = argv;
  if (command === 'request' && args.length >= 1 && args.length <= 2) {
    process.stdout.write(requestCode(args[0], args[1] === undefined ? 0 : Number(args[1])));
    return 0;
  }
  if (command === 'join' && args.length) {
    const text = joinParts(args.map(f => JSON.parse(fs.readFileSync(f, 'utf8'))));
    process.stdout.write(JSON.stringify(JSON.parse(text), null, 1) + '\n');
    return 0;
  }
  if (command === 'export' && args.length === 1) {
    const result = await exportSnapshot(JSON.parse(fs.readFileSync(args[0], 'utf8')));
    if (result.code) { process.stdout.write(result.code); return 0; }
    for (const e of result.errors) process.stderr.write(`${e.path}${e.nodeId ? ` [${e.nodeId}]` : ''}: ${e.message}\n`);
    return 1;
  }
  process.stderr.write(usage + '\n');
  return 2;
}

if (require.main === module) {
  main(process.argv.slice(2)).then(code => { process.exitCode = code; }, e => { process.stderr.write(`${e.message}\n`); process.exitCode = 1; });
}
module.exports = { exportSnapshot, joinParts, requestCode, hydrate };
