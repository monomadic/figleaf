/* The snapshot script and the CLI against the exporter itself. These are
 * synthetic: the "live" nodes are the reference fixtures with Figma's
 * methods put back, not Figma. A run against a real file is in
 * plugin/README.md under "Exporting without the plugin". */
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const { exportHierarchy } = require('./exporter');
const {
  FIGLEAF_SNAPSHOT_PROPS, FIGLEAF_SNAPSHOT_SEGMENT_FIELDS,
  figleafSnapshotHierarchy, figleafSnapshotText, figleafSnapshotSlice,
} = require('./snapshot');
const { exportSnapshot, joinParts, requestCode } = require('./cli');

const fixtures = fs.readdirSync(__dirname).filter(f => f.endsWith('-reference.json')).sort();
const load = f => JSON.parse(fs.readFileSync(path.join(__dirname, f), 'utf8'));
// A fixture as Figma would hand it over: text runs behind the method.
function live(n) {
  if (n.styledSegments) n.getStyledTextSegments = () => n.styledSegments;
  (n.children || []).forEach(live);
  return n;
}
// The image fixture holds a root and the pattern source it paints with.
const rootOf = j => (j.root && j.root.type ? j.root : j);
const noVariables = { variables: { getVariableByIdAsync: async () => null }, getNodeByIdAsync: async () => null };

test('the snapshot carries every node property and text field the exporter reads', () => {
  const reads = new Set(), fields = new Set(), textReads = new Set();
  function watch(n) {
    if (n.styledSegments) n.getStyledTextSegments = requested => { requested.forEach(f => fields.add(f)); return n.styledSegments; };
    if (n.children) n.children = n.children.map(watch);
    return new Proxy(n, {
      get(t, k, r) {
        if (typeof k === 'string') { reads.add(k); if (t.type === 'TEXT') textReads.add(k); }
        return Reflect.get(t, k, r);
      },
      has(t, k) { if (typeof k === 'string') reads.add(k); return Reflect.has(t, k); },
    });
  }
  for (const f of fixtures) exportHierarchy([watch(rootOf(load(f)))], 'SRGB');
  const carried = new Set([...FIGLEAF_SNAPSHOT_PROPS, 'children', 'getStyledTextSegments', 'styledSegments']);
  assert.deepEqual([...reads].filter(k => !carried.has(k)), [], 'add these to FIGLEAF_SNAPSHOT_PROPS');
  assert.deepEqual([...fields].filter(k => !FIGLEAF_SNAPSHOT_SEGMENT_FIELDS.includes(k)), [], 'add these to FIGLEAF_SNAPSHOT_SEGMENT_FIELDS');
  // The snapshot leaves out a text layer's glyph outlines (snapshot.js).
  assert.ok(!textReads.has('fillGeometry'), 'the exporter now reads text fillGeometry: stop skipping it');
});

test('every reference fixture survives snapshot, parts, join and export unchanged', async () => {
  for (const f of fixtures) {
    const direct = exportHierarchy([live(rootOf(load(f)))], 'SRGB');
    const source = load(f).source;
    const snapshot = await figleafSnapshotHierarchy(live(rootOf(load(f))), {
      ...noVariables,
      getNodeByIdAsync: async id => (source && source.id === id ? live(source) : null),
    });
    const text = figleafSnapshotText(snapshot);
    assert.match(text, /^[\x00-\x7e]*$/, `${f}: ASCII only`);
    // Small parts, so most fixtures arrive in several, out of order.
    const parts = [];
    for (let i = 0; i < Math.ceil(text.length / 4000); i++) parts.push(JSON.parse(JSON.stringify(figleafSnapshotSlice(text, i, 4000))));
    const joined = JSON.parse(joinParts(parts.reverse()));
    const viaCli = await exportSnapshot(joined);
    // Image paints need the plugin's decoder: the CLI refuses them, by name.
    if (JSON.stringify(load(f)).includes('"IMAGE"')) {
      assert.equal(viaCli.code, null);
      assert.match(viaCli.errors[0].message, /CLI cannot read image fills/);
      continue;
    }
    assert.deepEqual(viaCli, direct, f);
  }
});

test('mixed values travel as {"$mixed": true} and come back as the symbol', async () => {
  const root = live(load('text-runs-reference.json'));
  root.children[0].fontSize = Symbol('mixed');
  const direct = exportHierarchy([root], 'SRGB');
  const snapshot = await figleafSnapshotHierarchy(root, noVariables);
  assert.deepEqual(snapshot.root.children[0].fontSize, { $mixed: true });
  assert.deepEqual(await exportSnapshot(JSON.parse(figleafSnapshotText(snapshot))), direct);
});

test('bound variables are resolved at each consumer when the snapshot is taken', async () => {
  const resolverContext = { exportHierarchy, module: { exports: {} } };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'variables.js'), 'utf8'), resolverContext);
  const { exportWithVariables } = resolverContext.module.exports;
  const root = live(load('primitive-reference.json'));
  const target = root.children[0];
  const alias = { type: 'VARIABLE_ALIAS', id: 'VariableID:1:2' };
  target.fills = [{ ...target.fills[0], boundVariables: { color: alias } }];
  target.resolvedVariableModes = { 'VariableCollectionId:1:1': '1:0' };
  const lookups = [];
  const api = {
    getNodeByIdAsync: async () => null,
    variables: {
      async getVariableByIdAsync(id) {
        lookups.push(id);
        return {
          name: 'tone/fill', variableCollectionId: 'VariableCollectionId:1:1',
          resolveForConsumer: n => ({ resolvedType: 'COLOR', value: n === target ? target.fills[0].color : null }),
        };
      },
    },
  };
  const direct = await exportWithVariables([root], 'SRGB', api.variables);
  assert.deepEqual(direct.errors, []);
  lookups.length = 0;
  const snapshot = await figleafSnapshotHierarchy(root, api);
  assert.deepEqual(lookups, ['VariableID:1:2'], 'one lookup per variable');
  assert.deepEqual(Object.keys(snapshot.variables['VariableID:1:2'].resolved), [target.id]);
  const viaCli = await exportSnapshot(JSON.parse(figleafSnapshotText(snapshot)));
  assert.deepEqual(viaCli, direct);
  assert.match(viaCli.code, /Resolved binding: .*tone\/fill/);

  // A variable Figma could not find is recorded as missing and refused.
  const gone = await figleafSnapshotHierarchy(root, { ...api, variables: { getVariableByIdAsync: async () => null } });
  const refused = await exportSnapshot(JSON.parse(figleafSnapshotText(gone)));
  assert.equal(refused.code, null);
  assert.match(refused.errors[0].message, /unavailable/);
});

test('parts from two snapshots, or a missing part, are refused', () => {
  const a = figleafSnapshotText({ figleafSnapshot: 1, root: { name: 'a'.repeat(50) } });
  const b = figleafSnapshotText({ figleafSnapshot: 1, root: { name: 'b'.repeat(50) } });
  const n = figleafSnapshotSlice(a, 0, 40).parts;
  assert.ok(n > 1 && figleafSnapshotSlice(b, 0, 40).parts === n);
  const pa = [...Array(n).keys()].map(i => figleafSnapshotSlice(a, i, 40));
  const pb = [...Array(n).keys()].map(i => figleafSnapshotSlice(b, i, 40));
  assert.equal(joinParts(pa), a);
  assert.throws(() => joinParts([pa[0], ...pb.slice(1)]), /different snapshot/);
  assert.throws(() => joinParts(pa.slice(1)), new RegExp(`Missing part 0 of ${n}`));
  assert.throws(() => figleafSnapshotSlice(a, n, 40), /out of range/);
});

test('the request code runs as one Plugin API call and returns part 0', async () => {
  const root = live(load('stroke-reference.json'));
  const page = { type: 'PAGE', loaded: false, async loadAsync() { this.loaded = true; } };
  root.parent = page;
  const figma = {
    root: { documentColorProfile: 'SRGB' },
    getNodeByIdAsync: async id => (id === root.id ? root : null),
    variables: { getVariableByIdAsync: async () => null },
  };
  // Plain JavaScript with top-level await and return, as use_figma runs it.
  const run = new vm.Script(`(async () => {\n${requestCode(root.id)}\n})()`);
  const part = await run.runInNewContext({ figma, Error, JSON, Math, Number, Object, Array, Set, Map, Symbol });
  assert.equal(part.figleafSnapshotPart, 0);
  assert.ok(page.loaded, 'the node\'s page is loaded first');
  assert.ok(JSON.stringify(part).length < 48000);
  assert.throws(() => requestCode('not a node'), /not a Figma node id/);
});

test('the CLI prints Rust on success and layer paths on failure', () => {
  const dir = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'figleaf-cli-'));
  const ok = path.join(dir, 'ok.json');
  fs.writeFileSync(ok, JSON.stringify(load('stroke-reference.json')));
  const rust = execFileSync(process.execPath, [path.join(__dirname, 'cli.js'), 'export', ok], { encoding: 'utf8' });
  assert.match(rust, /pub fn draw_selection/);
  const bad = path.join(dir, 'bad.json');
  fs.writeFileSync(bad, JSON.stringify({ ...load('stroke-reference.json'), blendMode: 'NOT_A_MODE' }));
  let failed;
  try { execFileSync(process.execPath, [path.join(__dirname, 'cli.js'), 'export', bad], { encoding: 'utf8', stdio: 'pipe' }); }
  catch (e) { failed = e; }
  assert.equal(failed.status, 1);
  assert.match(failed.stderr, /\[355:242\]: /);
  fs.rmSync(dir, { recursive: true });
});
