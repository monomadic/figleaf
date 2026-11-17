/* Read-only verification of bindings. Figma's node/paint getters already expose
 * values in the consumer's mode; resolveForConsumer verifies that they are not
 * stale fallback values from an unavailable variable. Never assign to the node. */
async function exportWithVariables(selection, colorProfile, variables, resources = {}) {
  const verified = new Map(), lookups = new Map(), errors = [];
  let count = 0;
  async function visit(node, path, depth) {
    if (++count > 5000 || depth > 128) throw Error('Hierarchy exceeds the 5,000-node or 128-level limit.');
    if (node.visible === false) return;
    const aliases = new Map();
    function bindings(value, field) {
      if (!value || typeof value !== 'object') throw Error(`Malformed variable binding: ${field}.`);
      if (Array.isArray(value)) { value.forEach((v,i) => bindings(v, `${field}[${i}]`)); return; }
      if (value.type === 'VARIABLE_ALIAS') {
        if (typeof value.id !== 'string' || !value.id) throw Error(`Missing variable ID: ${field}.`);
        const fields = aliases.get(value.id) || [];
        fields.push(field); aliases.set(value.id, fields); return;
      }
      for (const [key, child] of Object.entries(value)) {
        if (child !== undefined) bindings(child, `${field}.${key}`);
      }
    }
    function paints(value, field) {
      if (!Array.isArray(value)) return; // The exporter reports mixed values.
      value.forEach((p,i) => {
        if (p.visible === false) return;
        bindings(p.boundVariables || {}, `${field}[${i}]`);
        paints(p.gradientStops, `${field}[${i}].stops`);
      });
    }
    try {
      // Paint/effect aliases are mirrored on nodes; inspect the visible entries
      // below so hidden paints do not require an available variable.
      const own = {...(node.boundVariables || {})};
      delete own.fills; delete own.strokes; delete own.effects;
      bindings(own, 'node');
      paints(node.fills, 'fills'); paints(node.strokes, 'strokes'); paints(node.effects, 'effects');
      for (const region of node.vectorNetwork?.regions || []) paints(region.fills, 'region.fills');
      if(node.type==='TEXT'&&node.getStyledTextSegments) {
        for(const [i,run] of node.getStyledTextSegments(['fills','boundVariables']).entries()) {bindings(run.boundVariables||{},`text.runs[${i}]`);paints(run.fills,`text.runs[${i}].fills`);}
      }
      const notes = [];
      for (const [id, fields] of aliases) {
        if (!lookups.has(id)) lookups.set(id, variables?.getVariableByIdAsync(id));
        const variable = await lookups.get(id);
        if (!variable) throw Error(`Variable ${id} is unavailable (${fields.join(', ')}).`);
        const result = variable.resolveForConsumer(node);
        const v = result.value;
        const valid = result.resolvedType === 'FLOAT' ? typeof v === 'number' && Number.isFinite(v)
          : result.resolvedType === 'STRING' ? typeof v === 'string'
          : result.resolvedType === 'BOOLEAN' ? typeof v === 'boolean'
          : result.resolvedType === 'COLOR' ? v && ['r','g','b'].every(k => typeof v[k] === 'number' && Number.isFinite(v[k]) && v[k]>=0 && v[k]<=1) && (v.a === undefined || typeof v.a === 'number' && Number.isFinite(v.a) && v.a>=0 && v.a<=1)
          : false;
        if (!valid) throw Error(`Variable ${id} did not resolve to a supported literal.`);
        const mode = node.resolvedVariableModes?.[variable.variableCollectionId];
        notes.push(`${fields.join(', ')}: ${variable.name} [${id}]${mode ? ` (mode ${mode})` : ''} = ${JSON.stringify(v)}`);
      }
      verified.set(node, notes);
    } catch (e) { errors.push({nodeId:node.id,path,message:e.message}); }
    for (const child of node.children || []) await visit(child, `${path} / ${child.name}`, depth+1);
  }
  try {
    if (selection.length === 1) await visit(selection[0], selection[0].name, 0);
    for (const source of resources.patterns?.values() || []) await visit(source,`Pattern source / ${source.name}`,0);
  } catch (e) { errors.push({path:'Selection',message:e.message}); }
  if (errors.length) return {code:null,errors};
  return exportHierarchy(selection, colorProfile, verified, resources);
}
if (typeof module !== 'undefined') module.exports = { exportWithVariables };
