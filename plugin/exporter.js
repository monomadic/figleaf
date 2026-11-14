/* Read-only Figma selection -> Rust. No Figma mutations, network, or storage.
 * Kept independent of the UI so the same validator is exercised by tests. */
function exportHierarchy(selection, colorProfile = 'SRGB') {
  const errors = [], lines = [], fonts = [];
  if (!['SRGB', 'LEGACY'].includes(colorProfile)) return { code: null, errors: [{ path: 'Document', message: `Unsupported color profile: ${colorProfile}. figleaf uses sRGB.` }] };
  const fail = (node, path, message) => errors.push({ nodeId: node.id, path, message });
  if (selection.length !== 1) return { code: null, errors: [{ path: 'Selection', message: 'Select exactly one hierarchy root.' }] };
  const number = value => {
    if (typeof value !== 'number' || !Number.isFinite(value) || Math.abs(value) > 1e7) throw Error('Invalid or out-of-range numeric value.');
    // Emit the shortest decimal that preserves figleaf's f32 value. Figma
    // returns doubles even for f32 colors; printing all digits fails Clippy.
    const f = Math.fround(value);
    let rounded = f;
    for (let digits = 1; digits <= 9; digits++) {
      const candidate = Number(f.toPrecision(digits));
      if (Math.fround(candidate) === f) { rounded = candidate; break; }
    }
    const s = String(Object.is(rounded, -0) ? 0 : rounded);
    return /[.eE]/.test(s) ? s : s + '.0';
  };
  const array = values => '[' + values.map(number).join(', ') + ']';
  const string = value => '"' + [...value].map(c => {
    if (c === '"' || c === '\\') return '\\' + c;
    const cp = c.codePointAt(0);
    return cp < 32 || cp === 127 || cp === 0x2028 || cp === 0x2029 ? '\\u{' + cp.toString(16) + '}' : c;
  }).join('') + '"';
  const comment = value => String(value).replace(/[\r\n\u2028\u2029]/g, ' ');
  const rect = r => `Rect::new(${r.map(number).join(', ')})`;
  const color = (c, opacity = 1) => array([c.r, c.g, c.b, (c.a ?? 1) * opacity]);
  const visible = list => {
    if (!Array.isArray(list)) throw Error('Mixed paints or effects are unsupported.');
    return list.filter(p => p.visible !== false);
  };
  const contains = (a, b) => a[0] <= b[0] && a[1] <= b[1] && a[0]+a[2] >= b[0]+b[2] && a[1]+a[3] >= b[1]+b[3];
  const combineClip = (a, b) => {
    if (!a) return b;
    if (a.radius || b.radius) {
      // A rectangular ancestor containing the entire rounded clip is redundant.
      if (!a.radius && contains(a.rect, b.rect)) return b;
      if (!b.radius && contains(b.rect, a.rect)) return a;
      throw Error('Intersecting rounded clips are unsupported.');
    }
    const x = Math.max(a.rect[0], b.rect[0]), y = Math.max(a.rect[1], b.rect[1]);
    return { rect: [x, y, Math.max(0, Math.min(a.rect[0]+a.rect[2], b.rect[0]+b.rect[2])-x), Math.max(0, Math.min(a.rect[1]+a.rect[3], b.rect[1]+b.rect[3])-y)], radius: 0 };
  };
  // Figma's documented absolute path command subset. Use resolved fillGeometry
  // so branched networks retain every filled region and hole. Never reconstruct
  // network topology from bounding boxes or silently turn strokes into fills.
  function vectorCode(n) {
    if (!Array.isArray(n.fillGeometry)) throw Error('Vector fill geometry is unavailable.');
    const geometry = [...n.fillGeometry.filter(p => p.windingRule !== 'NONE'), ...(n.vectorPaths || n.fillGeometry).filter(p => p.windingRule === 'NONE')];
    if (geometry.length > 256) throw Error('Vector exceeds 256 regions.');
    const paints = JSON.stringify(n.fills);
    for (const region of n.vectorNetwork?.regions || []) {
      if (region.fills && JSON.stringify(region.fills) !== paints) throw Error('Per-region vector paint overrides are unsupported.');
    }
    let commands = 0;
    const paths = [];
    for (const path of geometry) {
      const rule = { NONZERO: 'NonZero', EVENODD: 'EvenOdd', NONE: 'None' }[path.windingRule];
      if (!rule) throw Error('Unsupported vector winding rule.');
      if (typeof path.data !== 'string' || path.data.length > 1000000) throw Error('Invalid or oversized vector path.');
      const token = /[MLQCZ]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?/g;
      if (path.data.replace(token, '').replace(/[\s,]/g, '')) throw Error('Unsupported or malformed vector path command.');
      const tokens = path.data.match(token) || [];
      let code = `VectorPath::new(WindingRule::${rule})`, started = false;
      for (let i = 0; i < tokens.length;) {
        const op = tokens[i++];
        const arity = { M: 2, L: 2, Q: 4, C: 6, Z: 0 }[op];
        if (arity === undefined || i + arity > tokens.length) throw Error('Malformed vector path arguments.');
        if (!started && op !== 'M') throw Error('Vector subpath must start with M.');
        started = true;
        const values = tokens.slice(i, i + arity).map(Number);
        values.forEach(number);
        i += arity;
        if (++commands > 4096) throw Error('Vector exceeds 4096 commands.');
        const args = [];
        for (let j = 0; j < arity; j += 2) args.push(array(values.slice(j, j + 2)));
        code += `\n            .${{M:'move_to', L:'line_to', Q:'quadratic_to', C:'cubic_to', Z:'close'}[op]}(${args.join(', ')})`;
      }
      paths.push(code);
    }
    return `vec![${paths.join(',\n        ')}]`;
  }
  let count = 0;
  function visit(n, path, offset, clip, root = false, depth = 0) {
    if (++count > 5000 || depth > 128) throw Error('Hierarchy exceeds the 5,000-node or 128-level limit.');
    // Invisible layers have no output, including their descendants.
    if (n.visible === false) return;
    const before = errors.length;
    const reject = message => fail(n, path, message);
    let childClip = clip;
    let position = offset;
    try {
      const types = ['FRAME', 'COMPONENT', 'COMPONENT_SET', 'INSTANCE', 'GROUP', 'RECTANGLE', 'ELLIPSE', 'LINE', 'TEXT', 'VECTOR'];
      if (!types.includes(n.type)) reject(`Unsupported node type: ${n.type}.`);
      if (n.isMask) reject('Masks are unsupported.');
      if (Object.keys(n.boundVariables || {}).length) reject('Variable bindings are unsupported; use explicit values.');
      if (n.opacity !== undefined && n.opacity !== 1) reject('Layer opacity requires compositing and is unsupported. Use paint opacity.');
      if (n.blendMode && !['NORMAL', 'PASS_THROUGH'].includes(n.blendMode)) reject(`Unsupported layer blend mode: ${n.blendMode}.`);
      if (n.cornerSmoothing) reject('Corner smoothing is unsupported.');
      const transform = n.relativeTransform;
      if (!transform) throw Error('Missing relative transform.');
      const [a, c, tx] = transform[0], [b, d, ty] = transform[1];
      if (n.type !== 'LINE' && (a !== 1 || b !== 0 || c !== 0 || d !== 1)) reject('Rotation, reflection, scale, and skew are unsupported.');
      position = root ? [0, 0] : [offset[0] + tx, offset[1] + ty];
      const box = [...position, n.width, n.height];
      box.forEach(number);
      if (n.width < 0 || n.height < 0) reject('Negative dimensions are unsupported.');
      if (n.type === 'GROUP' && visible(n.effects || []).length) reject('Group effects require compositing and are unsupported.');
      if (n.children?.some(c => c.visible !== false) && visible(n.effects || []).length) reject('Effects on a hierarchy container require compositing and are unsupported.');
      const fills = visible(n.fills || []), strokes = visible(n.strokes || []), effects = visible(n.effects || []);
      for (const p of [...fills, ...strokes]) {
        if (Object.keys(p.boundVariables || {}).length) reject('Paint variable bindings are unsupported; use explicit values.');
        if (p.blendMode && p.blendMode !== 'NORMAL') reject(`Unsupported paint blend mode: ${p.blendMode}.`);
        if (!['SOLID', 'GRADIENT_LINEAR'].includes(p.type)) reject(`Unsupported paint: ${p.type}.`);
        if (p.type === 'GRADIENT_LINEAR') {
          if (p.gradientStops.length < 1 || p.gradientStops.length > 8) reject('Linear gradients require 1–8 stops.');
          const t = p.gradientTransform;
          if (!t || !t.flat().every(Number.isFinite) || Math.abs(t[0][0]*t[1][1]-t[0][1]*t[1][0]) < 1e-12) reject('Degenerate gradient transforms are unsupported.');
        }
      }
      if (strokes.length > 1) reject('Multiple strokes are unsupported.');
      if (strokes.some(p => p.type !== 'SOLID')) reject('Only solid strokes are supported.');
      if (strokes.length && n.dashPattern?.length) reject('Dashed strokes are unsupported.');
      if (strokes.length && n.strokeJoin && n.strokeJoin !== 'MITER') reject(`Unsupported stroke join: ${String(n.strokeJoin)}.`);
      for (const e of effects) {
        if (Object.keys(e.boundVariables || {}).length) reject('Effect variable bindings are unsupported; use explicit values.');
        if (!['DROP_SHADOW', 'INNER_SHADOW'].includes(e.type)) reject(`Unsupported effect: ${e.type}.`);
        if (e.blendMode && e.blendMode !== 'NORMAL') reject(`Unsupported effect blend mode: ${e.blendMode}.`);
      }
      const radii = 'topLeftRadius' in n ? [n.topLeftRadius, n.topRightRadius, n.bottomRightRadius, n.bottomLeftRadius] : [0,0,0,0];
      radii.forEach(number);
      if (n.type === 'ELLIPSE' && n.arcData && (n.arcData.startingAngle !== 0 || Math.abs(n.arcData.endingAngle - Math.PI*2) > 1e-6 || n.arcData.innerRadius !== 0)) reject('Ellipse arcs and rings are unsupported.');
      let expression = null;
      if (n.type === 'TEXT') {
        if (clip?.radius) reject('Rounded clipping of text is unsupported.');
        if (fills.length !== 1 || fills[0]?.type !== 'SOLID') reject('Text requires exactly one solid fill.');
        if (strokes.length) reject('Text strokes are unsupported.');
        if (typeof n.fontName !== 'object' || typeof n.fontSize !== 'number' || typeof n.letterSpacing !== 'object' || typeof n.lineHeight !== 'object') reject('Mixed text styles are unsupported.');
        if (n.textCase !== 'ORIGINAL' || n.textDecoration !== 'NONE') reject('Text case transformations and decorations are unsupported.');
        if (n.paragraphSpacing || n.paragraphIndent || n.hangingPunctuation || n.hangingList) reject('Paragraph spacing, indentation, and hanging text are unsupported.');
        if (n.textTruncation && n.textTruncation !== 'DISABLED' || n.maxLines != null) reject('Text truncation and maximum line counts are unsupported.');
        if (n.hasMissingFont) reject('The text uses a missing font.');
        if (n.getStyledTextSegments) {
          const segments = n.getStyledTextSegments(['fontName','fontSize','fills','textCase','textDecoration','letterSpacing','lineHeight','listOptions','indentation','openTypeFeatures']);
          if (segments.length > 1) reject('Mixed text runs are unsupported.');
          if (segments.some(s => s.listOptions?.type !== 'NONE' && s.listOptions?.type !== undefined || s.indentation || Object.keys(s.openTypeFeatures || {}).length)) reject('Lists, indentation, and OpenType overrides are unsupported.');
        }
        if (n.lineHeight?.unit === 'AUTO') reject('Automatic text line height is unsupported; set an explicit line height.');
        if (!['LEFT','CENTER','RIGHT'].includes(n.textAlignHorizontal) || !['TOP','CENTER','BOTTOM'].includes(n.textAlignVertical)) reject('Unsupported text alignment.');
        const text = n.characters;
        if (typeof text !== 'string') reject('Missing text content.');
        else {
          if (unescape(encodeURIComponent(text)).length > 512) reject('Text exceeds the 512-byte limit.');
          if ([...text].some(c => { const p = c.codePointAt(0); return c !== '\n' && !(p>=0x20&&p<=0x7e || p>=0xa0&&p<=0xac || p>=0xae&&p<=0x24f || p>=0x2010&&p<=0x2027 || p>=0x2030&&p<=0x205e || p>=0x20a0&&p<=0x20bf || p>=0x2100&&p<=0x22ff); })) reject('Text contains characters outside figleaf’s Latin text scope.');
        }
        const height = n.lineHeight?.unit === 'PERCENT' ? n.fontSize*n.lineHeight.value/100 : n.lineHeight?.value;
        const tracking = n.letterSpacing?.unit === 'PERCENT' ? n.fontSize*n.letterSpacing.value/100 : n.letterSpacing?.value;
        if (!(n.fontSize > 0 && n.fontSize <= 128 && height >= n.fontSize && height <= 256 && Math.abs(tracking) <= n.fontSize && n.width <= 2048)) reject('Text exceeds figleaf’s size, line-height, tracking, or width limits.');
        for (const type of ['DROP_SHADOW','INNER_SHADOW']) {
          if (effects.filter(e => e.type === type).length > 1) reject(`Text allows only one ${type}.`);
        }
        if (effects.some(e => e.spread || e.showShadowBehindNode)) reject('Text shadow spread and show-behind are unsupported.');
        if (errors.length === before && text) {
          const key = JSON.stringify(n.fontName);
          let font = fonts.findIndex(f => JSON.stringify(f) === key);
          if (font < 0) { font = fonts.length; fonts.push(n.fontName); }
          if (fonts.length > 64) reject('More than 64 fonts are unsupported.');
          const align = { LEFT:'Start', TOP:'Start', CENTER:'Center', RIGHT:'End', BOTTOM:'End' };
          expression = `Text::new(${string(text)}, ${rect(box)}).font(fonts[${font}]).size(${number(n.fontSize)}, ${number(height)}).letter_spacing(${number(tracking)}).color(${color(fills[0].color, fills[0].opacity)})`;
          expression += `.align(figleaf::paint::Align::${align[n.textAlignHorizontal]}, figleaf::paint::Align::${align[n.textAlignVertical]}).wrap()`;
          for (const e of effects) expression += `.${e.type === 'DROP_SHADOW' ? 'drop_shadow' : 'inner_shadow'}(${color(e.color)}, ${array([e.offset.x,e.offset.y])}, ${number(e.radius)})`;
          if (clip) expression += `.clip(${rect(clip.rect)})`;
        }
      } else if (n.type === 'LINE') {
        if (fills.length || strokes.length !== 1 || n.strokeCap !== 'ROUND' || effects.length) reject('Lines require one solid round-capped stroke, no fills, and no effects.');
        if (Math.abs(a*a+b*b-1) > 1e-6 || Math.abs(c*c+d*d-1)>1e-6 || Math.abs(a*c+b*d)>1e-6) reject('Scaled or skewed lines are unsupported.');
        if (errors.length === before) expression = `Shape::line(${array(position)}, ${array([position[0]+a*n.width,position[1]+b*n.width])}, ${number(n.strokeWeight)}).fill(${color(strokes[0].color,strokes[0].opacity)})`;
      } else if (n.type !== 'GROUP' && types.includes(n.type)) {
        if (n.type === 'VECTOR') {
          if (strokes.length) reject('Vector path strokes are unsupported (geometry task 3).');
          if (effects.length) reject('Vector path effects are unsupported.');
          expression = `Shape::vector(${rect(box)}, ${vectorCode(n)})`;
        } else {
          expression = `Shape::${n.type === 'ELLIPSE' ? 'ellipse' : 'rect'}(${rect(box)})`;
        }
        if (radii.some(r => r)) expression += `.radii(${array(radii)})`;
        // Figma's paint arrays are in drawing order, bottom to top.
        for (const p of fills) {
          if (p.type === 'SOLID') expression += `.fill(${color(p.color,p.opacity)})`;
          if (p.type === 'GRADIENT_LINEAR' && p.gradientTransform) {
            expression += `.gradient(LinearGradient::transform(${array(p.gradientTransform[0])})`;
            for (const s of p.gradientStops) expression += `.stop(${number(s.position)}, ${color(s.color)})`;
            expression += `.opacity(${number(p.opacity ?? 1)}))`;
          }
        }
        if (strokes.length === 1 && strokes[0].type === 'SOLID') {
          const p = strokes[0];
          const align = { INSIDE:'Inside', CENTER:'Center', OUTSIDE:'Outside' }[n.strokeAlign];
          if (!align) reject('Unsupported stroke alignment.');
          if ('strokeTopWeight' in n) expression += `.strokes(${array([n.strokeTopWeight,n.strokeRightWeight,n.strokeBottomWeight,n.strokeLeftWeight])}, ${color(p.color,p.opacity)}, StrokeAlign::${align})`;
          else expression += `.stroke(${number(n.strokeWeight)}, ${color(p.color,p.opacity)}, StrokeAlign::${align})`;
        }
        for (const e of effects) if (['DROP_SHADOW','INNER_SHADOW'].includes(e.type)) {
          expression += `.shadow(Shadow::${e.type === 'INNER_SHADOW' ? 'inner' : 'drop'}(${color(e.color)}, ${array([e.offset.x,e.offset.y])}, ${number(e.radius)}).spread(${number(e.spread ?? 0)})${e.showShadowBehindNode ? '.show_behind()' : ''})`;
        }
      }
      if (expression && n.type !== 'TEXT' && clip) expression += `.clip_rounded(${rect(clip.rect)}, ${number(clip.radius)})`;
      if (expression && errors.length === before) {
        lines.push(`    // ${comment(path)} [${comment(n.id)}]`, `    canvas.${n.type === 'TEXT' ? 'text' : 'shape'}(${expression});`);
      }
      if (n.clipsContent) {
        if (!radii.every(r => r === radii[0])) reject('Clipping with unequal corner radii is unsupported.');
        childClip = combineClip(clip, { rect: box, radius: radii[0] });
      }
    } catch (e) { reject(e.message); }
    let children = n.children || [];
    if (n.itemReverseZIndex) children = [...children].reverse();
    for (const child of children) visit(child, path + ' / ' + child.name, position, childClip, false, depth + 1);
  }
  try { visit(selection[0], selection[0].name, [0,0], null, true); }
  catch (e) { fail(selection[0], selection[0].name, e.message); }
  if (errors.length) return { code: null, errors };
  const preamble = [
    '// Generated by figleaf. Current dimensions and resolved values; no responsive layout or interactions.',
    '// Local origin is the selected root. Ancestors outside the selection are not included.',
    '// Supply matching fonts/styles through your text backend. Render at scale 0.5–4.',
    ...fonts.map((f,i) => `// fonts[${i}]: ${comment(f.family)} / ${comment(f.style)}`),
    '#[allow(unused_imports)]',
    'use figleaf::{Canvas, Rect, Shape, Text, Shadow, StrokeAlign, LinearGradient, VectorPath, WindingRule};',
    '', `pub fn draw_selection(canvas: &mut Canvas${fonts.length ? `, fonts: [usize; ${fonts.length}]` : ''}) {`,
  ];
  return { code: [...preamble, ...lines, '}',''].join('\n'), errors: [], fonts, nodeCount: count };
}
if (typeof module !== 'undefined') module.exports = { exportHierarchy };
