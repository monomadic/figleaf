/* Read-only Figma hierarchy -> JSON snapshot, for `node plugin/cli.js export`.
 *
 * This runs INSIDE Figma, in one Plugin API call: an agent sends this file's
 * text plus one `return await figleafSnapshotPart(...)` line through Figma's
 * MCP `use_figma` tool (`node plugin/cli.js request <node-id>` prints exactly
 * that code). It reads the node the exporter needs and nothing else, resolves
 * variable bindings where they are consumed, and never changes the document.
 *
 * A snapshot is the Plugin API's own values under the Plugin API's own names,
 * the shape of the `*-reference.json` fixtures, plus what the exporter would
 * otherwise ask Figma for: the resolved value of each bound variable at each
 * consuming node, and the nodes behind pattern paints. It is not a figleaf
 * scene format; the CLI only replays it into the exporter.
 *
 * Figma caps one call's result near 48 KB, so the text is returned in parts:
 * ASCII-only (non-ASCII characters are \u-escaped), each part a slice of it,
 * every part carrying the whole text's length and hash so `cli.js join` can
 * refuse a set that was not cut from one snapshot. */
const FIGLEAF_SNAPSHOT_VERSION = 1;

// Every node property the exporter reads. `snapshot.test.js` runs the
// exporter over the reference fixtures with every read recorded and fails
// if it reads a property missing here: add it here, not in the test.
const FIGLEAF_SNAPSHOT_PROPS = [
  'id', 'name', 'type', 'visible', 'opacity', 'blendMode', 'isMask', 'maskType',
  'relativeTransform', 'x', 'y', 'rotation', 'width', 'height', 'constraints',
  'fills', 'strokes', 'effects', 'gradientStops', 'fillGeometry', 'strokeGeometry', 'vectorPaths', 'vectorNetwork',
  'strokeWeight', 'strokeAlign', 'strokeCap', 'strokeJoin', 'strokeMiterLimit', 'dashPattern',
  'strokeTopWeight', 'strokeRightWeight', 'strokeBottomWeight', 'strokeLeftWeight',
  'complexStrokeProperties', 'variableWidthStrokeProperties',
  'cornerRadius', 'cornerSmoothing', 'topLeftRadius', 'topRightRadius', 'bottomLeftRadius', 'bottomRightRadius',
  'arcData', 'pointCount', 'innerRadius', 'booleanOperation', 'clipsContent',
  'layoutMode', 'layoutWrap', 'layoutPositioning', 'layoutSizingHorizontal', 'layoutSizingVertical',
  'primaryAxisAlignItems', 'counterAxisAlignItems', 'counterAxisAlignContent', 'counterAxisSpacing',
  'itemSpacing', 'itemReverseZIndex', 'strokesIncludedInLayout',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'minWidth', 'maxWidth', 'minHeight', 'maxHeight',
  'characters', 'fontName', 'fontSize', 'lineHeight', 'letterSpacing', 'textCase', 'textAlignHorizontal',
  'textAlignVertical', 'textAutoResize', 'textTruncation', 'maxLines', 'textDecoration', 'textDecorationColor',
  'textDecorationStyle', 'textDecorationOffset', 'textDecorationThickness', 'textDecorationSkipInk',
  'paragraphIndent', 'paragraphSpacing', 'listSpacing', 'listOptions', 'indentation', 'openTypeFeatures',
  'hangingList', 'hangingPunctuation', 'hasMissingFont', 'boundVariables', 'resolvedVariableModes',
];

// The run fields the exporter asks `getStyledTextSegments` for (the widest
// of its calls); stored as the node's `styledSegments`, like the fixtures.
const FIGLEAF_SNAPSHOT_SEGMENT_FIELDS = [
  'fontName', 'fontSize', 'fills', 'textCase', 'textDecoration', 'textDecorationStyle',
  'textDecorationOffset', 'textDecorationThickness', 'textDecorationColor', 'textDecorationSkipInk',
  'letterSpacing', 'lineHeight', 'listOptions', 'listSpacing', 'indentation', 'paragraphSpacing',
  'paragraphIndent', 'openTypeFeatures', 'boundVariables',
];

// Plain data from a Plugin API value: Figma's `mixed` symbol becomes
// {"$mixed": true}; functions and undefined are left out.
function figleafSnapshotValue(v) {
  if (typeof v === 'symbol') return { $mixed: true };
  if (v === null || typeof v !== 'object') return v;
  if (Array.isArray(v)) return v.map(figleafSnapshotValue);
  const out = {};
  for (const k of Object.keys(v)) {
    const c = figleafSnapshotValue(v[k]);
    if (c !== undefined && typeof c !== 'function') out[k] = c;
  }
  return out;
}

// Every variable alias in a plain value: {type: 'VARIABLE_ALIAS', id}.
function figleafSnapshotAliases(v, out) {
  if (!v || typeof v !== 'object') return out;
  if (Array.isArray(v)) { v.forEach(x => figleafSnapshotAliases(x, out)); return out; }
  if (v.type === 'VARIABLE_ALIAS' && typeof v.id === 'string') out.add(v.id);
  for (const k of Object.keys(v)) if (k !== 'children') figleafSnapshotAliases(v[k], out);
  return out;
}

// `api` is the `figma` global, or anything with its `variables` and
// `getNodeByIdAsync` (the tests pass a fake).
async function figleafSnapshotHierarchy(root, api) {
  const variables = {}, sources = {}, lookups = new Map();
  let count = 0;
  async function resolveBindings(live, plain) {
    for (const id of figleafSnapshotAliases(plain, new Set())) {
      if (!lookups.has(id)) lookups.set(id, api.variables.getVariableByIdAsync(id));
      const variable = await lookups.get(id);
      const entry = variables[id] || (variables[id] = variable
        ? { name: variable.name, variableCollectionId: variable.variableCollectionId, resolved: {} }
        : null);
      // An unavailable variable stays `null`: the exporter reports it.
      if (entry) entry.resolved[live.id] = figleafSnapshotValue(variable.resolveForConsumer(live));
    }
  }
  async function node(live, depth) {
    if (++count > 5000 || depth > 128) throw Error('Hierarchy exceeds the 5,000-node or 128-level limit.');
    // A hidden layer draws nothing and the exporter skips it whole.
    if (live.visible === false) return { id: live.id, name: live.name, type: live.type, visible: false };
    const out = {};
    for (const k of FIGLEAF_SNAPSHOT_PROPS) {
      // A text layer's fill geometry is its glyph outlines: most of the
      // snapshot, and never read (the exporter emits text, not glyphs).
      if (k === 'fillGeometry' && live.type === 'TEXT') continue;
      let v;
      try { v = live[k]; } catch (e) { continue; }
      if (v === undefined || typeof v === 'function') continue;
      out[k] = figleafSnapshotValue(v);
    }
    if (live.type === 'TEXT' && typeof live.getStyledTextSegments === 'function') {
      out.styledSegments = figleafSnapshotValue(live.getStyledTextSegments(FIGLEAF_SNAPSHOT_SEGMENT_FIELDS));
    }
    await resolveBindings(live, out);
    for (const p of [...(Array.isArray(out.fills) ? out.fills : []), ...(Array.isArray(out.strokes) ? out.strokes : [])]) {
      if (p.type === 'PATTERN' && p.visible !== false && !(p.sourceNodeId in sources)) {
        sources[p.sourceNodeId] = null; // claimed first: a cycle ends here
        const source = await api.getNodeByIdAsync(p.sourceNodeId);
        sources[p.sourceNodeId] = source ? await node(source, depth + 1) : null;
      }
    }
    if (Array.isArray(live.children)) {
      out.children = [];
      for (const c of live.children) out.children.push(await node(c, depth + 1));
    }
    return out;
  }
  const tree = await node(root, 0);
  return {
    figleafSnapshot: FIGLEAF_SNAPSHOT_VERSION,
    colorProfile: api.root ? api.root.documentColorProfile : 'SRGB',
    root: tree,
    variables,
    sources,
  };
}

// The snapshot as ASCII JSON: one byte per character, so a part's size is
// its length.
function figleafSnapshotText(snapshot) {
  return JSON.stringify(snapshot).replace(/[\u007f-\uffff]/g, c => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
}

// FNV-1a over the text: enough to tell parts of two snapshots apart.
function figleafSnapshotHash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, '0');
}

// One part of the snapshot text. The JSON of a part escapes the slice's
// quotes and backslashes again, so 20,000 characters stay under Figma's
// ~48 KB result limit whatever they contain.
function figleafSnapshotSlice(text, part, size = 20000) {
  const parts = Math.max(1, Math.ceil(text.length / size));
  if (!Number.isInteger(part) || part < 0 || part >= parts) throw Error(`Part ${part} is out of range: the snapshot has ${parts}.`);
  return { figleafSnapshotPart: part, parts, length: text.length, hash: figleafSnapshotHash(text), data: text.slice(part * size, (part + 1) * size) };
}

// The entry point for a Plugin API call: snapshot `nodeId` and return one part.
async function figleafSnapshotPart(nodeId, part = 0, size = 20000) {
  const root = await figma.getNodeByIdAsync(nodeId);
  if (!root) throw Error(`No node ${nodeId} in this file.`);
  let page = root;
  while (page && page.type !== 'PAGE') page = page.parent;
  if (page && typeof page.loadAsync === 'function') await page.loadAsync();
  return figleafSnapshotSlice(figleafSnapshotText(await figleafSnapshotHierarchy(root, figma)), part, size);
}

if (typeof module !== 'undefined') module.exports = {
  FIGLEAF_SNAPSHOT_VERSION, FIGLEAF_SNAPSHOT_PROPS, FIGLEAF_SNAPSHOT_SEGMENT_FIELDS,
  figleafSnapshotHierarchy, figleafSnapshotText, figleafSnapshotHash, figleafSnapshotSlice,
};
