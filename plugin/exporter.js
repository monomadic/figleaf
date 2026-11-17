/* Read-only Figma selection -> Rust. No Figma mutations, network, or storage.
 * Kept independent of the UI so the same validator is exercised by tests. */
function exportHierarchy(selection, colorProfile = 'SRGB', verifiedBindings = new Map(), resources = {}) {
  const errors = [], lines = [], fonts = [];
  let maskIndex = 0, compositeIndex=0;
  const imageNames=new Map(), patternNames=new Map(), patternStack=new Set();
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
  const blendModes={NORMAL:'Normal',PASS_THROUGH:'PassThrough',MULTIPLY:'Multiply',SCREEN:'Screen',OVERLAY:'Overlay',DARKEN:'Darken',LIGHTEN:'Lighten',COLOR_DODGE:'ColorDodge',COLOR_BURN:'ColorBurn',HARD_LIGHT:'HardLight',SOFT_LIGHT:'SoftLight',DIFFERENCE:'Difference',EXCLUSION:'Exclusion',HUE:'Hue',SATURATION:'Saturation',COLOR:'Color',LUMINOSITY:'Luminosity',LINEAR_BURN:'LinearBurn',LINEAR_DODGE:'LinearDodge'};
  const blended = p=>p.visible!==false && p.blendMode && p.blendMode!=='NORMAL';
  const needsComposite=n=>n.visible!==false && ((!n.isMask&&n.opacity!==undefined&&n.opacity!==1)||(!['NORMAL','PASS_THROUGH',undefined].includes(n.blendMode))||[...(Array.isArray(n.fills)?n.fills:[]),...(Array.isArray(n.strokes)?n.strokes:[])].some(blended)||(Array.isArray(n.effects)&&n.effects.some(e=>e.visible!==false&&(blended(e)||n.children?.length||['LAYER_BLUR','BACKGROUND_BLUR'].includes(e.type))))||(n.children||[]).some(needsComposite));
  const shadowCode=e=>`Shadow::${e.type==='INNER_SHADOW'?'inner':'drop'}(${color(e.color)}, ${array([e.offset.x,e.offset.y])}, ${number(e.radius)}).spread(${number(e.spread ?? 0)})${e.showShadowBehindNode?'.show_behind()':''}`;
  const gradientCode = p => (p.type==='GRADIENT_LINEAR' ? `LinearGradient::transform(${array(p.gradientTransform[0])})` : `Gradient::${p.type.slice(9).toLowerCase()}([${p.gradientTransform.map(array).join(', ')}])`) + p.gradientStops.map(s=>`.stop(${number(s.position)}, ${color(s.color)})`).join('') + `.opacity(${number(p.opacity ?? 1)})`;
  function imageCode(p) {
    const pixels=resources.images?.get(p.imageHash);
    if (!pixels || !Number.isInteger(pixels.width) || !Number.isInteger(pixels.height) || pixels.width<=0 || pixels.height<=0 || pixels.width*pixels.height>1000000 || pixels.rgba.length!==pixels.width*pixels.height*4 || Array.from(pixels.rgba).some(v=>!Number.isInteger(v)||v<0||v>255)) throw Error('Image pixels are missing, invalid or exceed the 1-million-pixel export limit.');
    if (!imageNames.has(p.imageHash)) {const name=`image_${imageNames.size}`;imageNames.set(p.imageHash,name);lines.push(`    let ${name} = Image::new([${pixels.width}, ${pixels.height}], vec![${Array.from(pixels.rgba).join(',')}]);`);}
    const mode=p.scaleMode==='CROP' ? `ImageScaleMode::Crop(${matrix(p.imageTransform)})` : p.scaleMode==='TILE' ? `ImageScaleMode::Tile(${number(p.scalingFactor ?? 1)})` : `ImageScaleMode::${p.scaleMode==='FIT'?'Fit':'Fill'}`;
    return `Paint::Image(ImagePaint::new(${imageNames.get(p.imageHash)}.clone()).scale_mode(${mode}).rotation(${number(p.rotation ?? 0)}).opacity(${number(p.opacity ?? 1)}))`;
  }
  function patternCode(p) {
    const source=resources.patterns?.get(p.sourceNodeId);
    if (!source) throw Error('Pattern source is unavailable.');
    if(source.visible===false || !(source.width>0 && source.height>0))throw Error('Pattern source must be visible and have nonzero dimensions.');
    if (patternStack.has(p.sourceNodeId) || patternStack.size>=8) throw Error('Cyclic or deeply nested pattern source.');
    if (!patternNames.has(p.sourceNodeId)) {
      if(patternNames.size>=32) throw Error('More than 32 pattern sources.');
      const name=`pattern_${patternNames.size}`;patternNames.set(p.sourceNodeId,name);patternStack.add(p.sourceNodeId);
      const m=source.relativeTransform;
      if (!m || m[0][0]!==1 || m[0][1]!==0 || m[1][0]!==0 || m[1][1]!==1) throw Error('Transformed pattern source roots are unsupported; use an untransformed containing frame.');
      lines.push(`    let mut ${name} = Canvas::new();`);
      visit(source,`Pattern source / ${source.name}`,identity,null,true,0,name);
      if(needsComposite(source))lines.push(`    let ${name}_content = std::mem::take(&mut ${name});`, `    ${name}.composite(Composite::new(${name}_content));`);
      patternStack.delete(p.sourceNodeId);
    }
    const tile={RECTANGULAR:'Rectangular',HORIZONTAL_HEXAGONAL:'HorizontalHexagonal',VERTICAL_HEXAGONAL:'VerticalHexagonal'}[p.tileType];
    const alignment={START:'Start',CENTER:'Center',END:'End'}[p.horizontalAlignment];
    const vertical={START:'Start',CENTER:'Center',END:'End'}[p.verticalAlignment ?? 'START'];
    if (!vertical || !tile || !alignment || !p.spacing || p.spacing.x<0 || p.spacing.y<0 || !(p.scalingFactor>0)) throw Error('Unsupported pattern tiling, alignment, spacing or scale.');
    return `Paint::Pattern(Pattern::new(${patternNames.get(p.sourceNodeId)}.clone(), ${array([source.width,source.height])}).scaling_factor(${number(p.scalingFactor)}).spacing(${array([p.spacing.x,p.spacing.y])}).tile_type(TileType::${tile}).horizontal_alignment(PatternAlignment::${alignment}).vertical_alignment(PatternAlignment::${vertical}).opacity(${number(p.opacity ?? 1)}))`;
  }
  const paintCode = p => p.type==='IMAGE' ? imageCode(p) : p.type==='PATTERN' ? patternCode(p) : p.type==='SOLID' ? `Paint::Solid(${color(p.color,p.opacity)})` : p.type.startsWith('GRADIENT_') ? `Paint::${p.type==='GRADIENT_LINEAR'?'Linear':'Gradient'}(${gradientCode(p)})` : (()=>{throw Error('Unsupported stroke paint.');})();
  const strokeColor = p => p.type==='SOLID' ? color(p.color,p.opacity) : '[1.0, 1.0, 1.0, 1.0]';
  const visible = list => {
    if (!Array.isArray(list)) throw Error('Mixed paints or effects are unsupported.');
    return list.filter(p => p.visible !== false);
  };
  const identity = [[1,0,0],[0,1,0]];
  const matrix = m => `Transform::new([${m.map(array).join(', ')}])`;
  function inverse(m) {
    if (!Array.isArray(m) || m.length !== 2 || m.some(r => !Array.isArray(r) || r.length !== 3)) throw Error('Invalid affine transform.');
    const [[a,c,x],[b,d,y]] = m.map(r => r.map(v => { number(v); return Math.fround(v); }));
    const det = a*d-b*c;
    if (!Number.isFinite(det) || det === 0) throw Error('Non-finite or singular transform.');
    const result = [[d/det,-c/det,(c*y-d*x)/det],[-b/det,a/det,(b*x-a*y)/det]];
    result.flat().forEach(number);
    return result;
  }
  const maxScale = m => {
    const [[a,c],[b,d]]=m;const trace=a*a+b*b+c*c+d*d;
    return Math.sqrt((trace+Math.sqrt((a*a+b*b-c*c-d*d)**2+4*(a*c+b*d)**2))/2);
  };
  const compose = (parent,child) => parent.map(r => [r[0]*child[0][0]+r[1]*child[1][0],r[0]*child[0][1]+r[1]*child[1][1],r[0]*child[0][2]+r[1]*child[1][2]+r[2]]);
  // Figma's documented absolute path command subset. Use resolved fillGeometry
  // so branched networks retain every filled region and hole. Never reconstruct
  // network topology from bounding boxes or silently turn strokes into fills.
  function vectorCode(n, resolvedStroke = false) {
    if (resolvedStroke && (!Array.isArray(n.strokeGeometry) || n.strokeGeometry.some(p=>p.windingRule==='NONE'))) throw Error('Resolved stroke geometry is unavailable or unfilled.');
    if (!Array.isArray(n.fillGeometry)) throw Error('Vector fill geometry is unavailable.');
    const geometry = resolvedStroke ? n.strokeGeometry : [...n.fillGeometry.filter(p => p.windingRule !== 'NONE'), ...(n.vectorPaths || n.fillGeometry).filter(p => p.windingRule === 'NONE')];
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
  const caps={NONE:'Butt',SQUARE:'Square',ROUND:'Round',ARROW_LINES:'ArrowLines',ARROW_EQUILATERAL:'ArrowEquilateral',TRIANGLE_FILLED:'TriangleFilled',DIAMOND_FILLED:'DiamondFilled',CIRCLE_FILLED:'CircleFilled'};
  // Trace each non-branching network edge once. vectorPaths is incomplete for
  // networks, so it is never used as a substitute for stroke centerlines.
  function strokeCode(n) {
    const network=n.vectorNetwork;
    if (!network || !Array.isArray(network.vertices) || !Array.isArray(network.segments)) throw Error('Stroke vector network is unavailable.');
    const {vertices,segments}=network;
    if (segments.length>4096 || vertices.length>4096) throw Error('Stroke network exceeds 4096 segments/vertices.');
    const adjacency=vertices.map(()=>[]);
    vertices.forEach(v=>{
      number(v.x);number(v.y);
      if (v.cornerRadius || n.cornerRadius) throw Error('Rounded vector vertices are unsupported.');
      if (v.strokeCap && !caps[v.strokeCap]) throw Error('Unsupported endpoint decoration.');
      if (v.strokeJoin && v.strokeJoin!==n.strokeJoin) throw Error('Per-vertex stroke joins are unsupported.');
    });
    segments.forEach((e,i)=>{
      for(const v of [e.start,e.end]) {
        if (!Number.isInteger(v) || !adjacency[v]) throw Error('Invalid stroke network vertex.');
        adjacency[v].push(i);
      }
      for(const t of [e.tangentStart,e.tangentEnd]) if(t){number(t.x);number(t.y);}
    });
    if(adjacency.some(a=>a.length===0)) throw Error('Isolated stroke vertices are unsupported.');
    if(adjacency.some(a=>a.length>2)) throw Error('Branching stroke networks are unsupported.');
    const used=new Set(),paths=[]; let commandCount=0;
    const starts=[...vertices.keys()].sort((a,b)=>adjacency[a].length-adjacency[b].length);
    for(const start of starts) {
      if(!adjacency[start].some(e=>!used.has(e))) continue;
      commandCount++;
      let at=start,code=`VectorPath::new(WindingRule::NonZero).move_to(${array([vertices[start].x,vertices[start].y])})`;
      while(true) {
        const index=adjacency[at].find(e=>!used.has(e)); if(index===undefined) break;
        commandCount++;
        used.add(index); const e=segments[index],forward=e.start===at,next=forward?e.end:e.start;
        const a=vertices[at],b=vertices[next],ta=(forward?e.tangentStart:e.tangentEnd)||{x:0,y:0},tb=(forward?e.tangentEnd:e.tangentStart)||{x:0,y:0};
        code+=ta.x || ta.y || tb.x || tb.y
          ? `.cubic_to(${array([a.x+ta.x,a.y+ta.y])}, ${array([b.x+tb.x,b.y+tb.y])}, ${array([b.x,b.y])})`
          : `.line_to(${array([b.x,b.y])})`;
        at=next; if(at===start){commandCount++;code+='.close()';break;}
      }
      if(at!==start) {
        const a=caps[vertices[start].strokeCap ?? n.strokeCap],b=caps[vertices[at].strokeCap ?? n.strokeCap];
        if(!a || !b) throw Error('Missing or unsupported endpoint caps.');
        code+=`.start_cap(StrokeCap::${a}).end_cap(StrokeCap::${b})`;
      }
      paths.push(code);
    }
    if(commandCount>4096) throw Error('Stroke network exceeds 4096 commands.');
    if(paths.length>256) throw Error('Stroke network exceeds 256 contours.');
    return `vec![${paths.join(', ')}]`;
  }
  let count = 0;
  // Operand paint is not drawn, but bound values still need verification and
  // provenance. Keep hierarchy limits even when resolved geometry is one item.
  function operands(n, path, depth) {
    if (++count > 5000 || depth > 128) throw Error('Hierarchy exceeds the 5,000-node or 128-level limit.');
    if (n.visible === false) return;
    const bound = v => v && typeof v==='object' && (Object.keys(v.boundVariables || {}).length || (v.gradientStops || []).some(bound));
    if (!verifiedBindings.has(n) && (bound(n) || [...(Array.isArray(n.fills)?n.fills:[]),...(Array.isArray(n.strokes)?n.strokes:[]),...(Array.isArray(n.effects)?n.effects:[])].some(bound))) fail(n,path,'Boolean operand bindings must be resolved before export.');
    for (const note of verifiedBindings.get(n) || []) lines.push(`    // Resolved operand binding (${comment(path)}): ${comment(note)}`);
    for (const child of n.children || []) operands(child,path+' / '+child.name,depth+1);
  }
  function visit(n, path, parentTransform, clip, root = false, depth = 0, target = 'canvas', asMask = false, maskDepth = 0, compositeDepth = 0) {
    if (++count > 5000 || depth > 128) throw Error('Hierarchy exceeds the 5,000-node or 128-level limit.');
    // Invisible layers have no output, including their descendants.
    if (n.visible === false) return;
    const before = errors.length;
    const reject = message => fail(n, path, message);
    let childClip = clip;
    let world = parentTransform;
    let scope=null;
    const outerTarget=target;
    try {
      const types = ['FRAME', 'COMPONENT', 'COMPONENT_SET', 'INSTANCE', 'GROUP', 'RECTANGLE', 'ELLIPSE', 'LINE', 'TEXT', 'VECTOR', 'POLYGON', 'STAR', 'BOOLEAN_OPERATION'];
      const boolean = n.type === 'BOOLEAN_OPERATION';
      if (boolean && !['UNION','SUBTRACT','INTERSECT','EXCLUDE'].includes(n.booleanOperation)) reject('Unsupported boolean operation.');
      if (!types.includes(n.type)) reject(`Unsupported node type: ${n.type}.`);
      if (n.isMask && !asMask) reject('Select the mask’s containing hierarchy, not the mask alone.');
      if (asMask && !['VECTOR','ALPHA','LUMINANCE'].includes(n.maskType)) reject('Unsupported mask type.');
      if (maskDepth > 8) reject('Mask nesting exceeds eight levels.');
      if (asMask && n.maskType==='VECTOR') {
        const unsupported = (node, root=true) => node.visible !== false && (node.type==='TEXT' || (!root && node.isMask) || (!root && needsComposite(node)) || (node.children || []).some(c=>unsupported(c,false)));
        if (unsupported(n)) reject('Vector masks containing text or nested masks require resolved outlines.');
      }
      if (Object.keys(n.boundVariables || {}).length && !verifiedBindings.has(n)) reject('Variable bindings must be resolved before export.');
      if(n.opacity!==undefined && (!Number.isFinite(n.opacity)||n.opacity<0||n.opacity>1))reject('Invalid layer opacity.');
      if (n.blendMode && !blendModes[n.blendMode]) reject(`Unsupported layer blend mode: ${n.blendMode}.`);
      const smoothing=n.cornerSmoothing ?? 0;
      if (typeof smoothing!=='number' || !Number.isFinite(smoothing) || smoothing<0 || smoothing>1) reject('Invalid corner smoothing.');
      const smoothed=smoothing>0;
      let resolved=boolean || smoothed;
      let styledStroke=false, strokeNote=null;
      const transform = n.relativeTransform;
      if (!transform) throw Error('Missing relative transform.');
      inverse(transform);
      const local = root ? transform.map(r=>[r[0],r[1],0]) : transform;
      world = compose(parentTransform,local);
      inverse(world);
      const box = [0, 0, n.width, n.height];
      box.forEach(number);
      if (n.width < 0 || n.height < 0) reject('Negative dimensions are unsupported.');
      const segments=n.type==='TEXT'&&n.getStyledTextSegments?n.getStyledTextSegments(['fontName','fontSize','fills','textCase','textDecoration','letterSpacing','lineHeight','listOptions','indentation','paragraphSpacing','paragraphIndent','openTypeFeatures','boundVariables']):null;
      const mixedRuns=segments&&segments.length>1?segments:null;
      const fills = mixedRuns?mixedRuns.flatMap(s=>visible(s.fills||[])):visible(n.fills || []), strokes = visible(n.strokes || []);
      const allEffects=visible(n.effects || []);
      const containerEffects=!!n.children?.length || n.type==='GROUP' || allEffects.some(blended);
      const scopeEffects=containerEffects || allEffects.some(e=>['LAYER_BLUR','BACKGROUND_BLUR'].includes(e.type));
      const effects=containerEffects?[]:allEffects.filter(e=>!['LAYER_BLUR','BACKGROUND_BLUR'].includes(e.type));
      for(const e of allEffects){if(Object.keys(e.boundVariables||{}).length&&!verifiedBindings.has(n))reject('Effect variable bindings must be resolved before export.');if(!['DROP_SHADOW','INNER_SHADOW','LAYER_BLUR','BACKGROUND_BLUR'].includes(e.type))reject(`Unsupported effect: ${e.type}.`);if(!blendModes[e.blendMode ?? 'NORMAL']||e.blendMode==='PASS_THROUGH')reject('Unsupported effect blend mode.');}
      for(const type of ['LAYER_BLUR','BACKGROUND_BLUR']) {
        const blurs=allEffects.filter(e=>e.type===type);
        if(blurs.length>1)reject(`Multiple ${type} effects are unsupported.`);
        for(const e of blurs) {
          if(e.blurType!==undefined&&e.blurType!=='NORMAL')reject('Progressive or unknown blur types are unsupported.');
          if(!Number.isFinite(e.radius)||e.radius<0||e.radius*maxScale(world)*1.5>128)reject('Blur radius exceeds filter limits.');
        }
      }
      const isolated=n.children?.length && n.blendMode==='NORMAL' && (n.children||[]).some(needsComposite);
      if((!asMask && n.opacity!==undefined && n.opacity!==1)||(!['NORMAL','PASS_THROUGH',undefined].includes(n.blendMode))||(scopeEffects&&allEffects.length)||isolated){
        if(asMask&&n.maskType==='VECTOR')reject('Vector masks cannot contain composited effects or blend scopes.');
        if(compositeDepth+maskDepth+patternStack.size>=8)reject('Combined compositing nesting exceeds eight levels.');
        compositeDepth++;
        const name=`composite_${compositeIndex++}`;
        scope={name,clips:clip||[],effects:scopeEffects?allEffects.filter(e=>!effects.includes(e)):[],mode:blendModes[n.blendMode ?? (n.children?.length?'PASS_THROUGH':'NORMAL')]};
        if(n.type!=='TEXT'&&!n.children?.length&&['NORMAL',undefined].includes(n.blendMode)&&[...fills,...strokes].some(blended))scope.mode='PassThrough';
        lines.push(`    let mut ${name} = Canvas::new();`);target=name;
        if(allEffects.some(e=>e.type==='LAYER_BLUR')) {clip=null;childClip=null;}
      }
      if (n.type==='LINE' && (strokes.length>1 || strokes.some(p=>p.type!=='SOLID'||blended(p)))) {
        resolved=true;
        if (n.strokeWeight>0 && (!Array.isArray(n.strokeGeometry)||!n.strokeGeometry.some(p=>typeof p.data==='string'&&/[LQC]/.test(p.data)))) reject('Painted line stroke outlines are unavailable.');
      }
      for (const p of [...fills, ...strokes]) {
        if ([p, ...(p.gradientStops || [])].some(v => Object.keys(v.boundVariables || {}).length) && !verifiedBindings.has(n)) reject('Paint variable bindings must be resolved before export.');
        if (p.blendMode && (!blendModes[p.blendMode]||p.blendMode==='PASS_THROUGH')) reject(`Unsupported paint blend mode: ${p.blendMode}.`);
        if (!['SOLID', 'GRADIENT_LINEAR', 'GRADIENT_RADIAL', 'GRADIENT_ANGULAR', 'GRADIENT_DIAMOND', 'IMAGE', 'PATTERN'].includes(p.type)) reject(`Unsupported paint: ${p.type}.`);
        if (['IMAGE','PATTERN'].includes(p.type) && (p.opacity!==undefined && (!Number.isFinite(p.opacity)||p.opacity<0||p.opacity>1))) reject('Invalid image/pattern opacity.');
        if (p.type==='IMAGE') {
          if (!['FILL','FIT','CROP','TILE'].includes(p.scaleMode)) reject('Unsupported image scale mode.');
          if (Object.values(p.filters || {}).some(v=>v!==0)) reject('Image adjustments are unsupported.');
          if (p.rotation!==undefined && (!Number.isFinite(p.rotation)||p.rotation%90!==0)) reject('Image rotation must be a multiple of 90 degrees.');
          if (p.scaleMode==='CROP') {try {inverse(p.imageTransform);}catch{reject('Invalid image crop transform.');} if(p.rotation)reject('Crop rotation must be in the image transform.');}
          if (p.scaleMode==='TILE' && !(p.scalingFactor>0)) reject('Invalid image tile scale.');
          if (!(n.width>0 && n.height>0)) reject('Images require nonzero paint bounds.');
        }
        if (p.type.startsWith('GRADIENT_')) {
          if (p.gradientStops.length < 1 || p.gradientStops.length > 64) reject('Gradients require 1–64 stops.');
          const t = p.gradientTransform;
          if (!t || !t.flat().every(Number.isFinite) || Math.abs(t[0][0]*t[1][1]-t[0][1]*t[1][0]) < 1e-12) reject('Degenerate gradient transforms are unsupported.');
          if (p.type!=='GRADIENT_LINEAR' && (n.width===0 || n.height===0)) reject('Gradient spans a zero-sized dimension.');
          if (t && ((n.width===0 && t[0][0]!==0) || (n.height===0 && t[0][1]!==0))) reject('Gradient axis spans a zero-sized dimension.');
        }
      }
      if (strokes.length) {
        const profile=n.variableWidthStrokeProperties, complex=n.complexStrokeProperties;
        const nonuniform=profile && profile.widthProfile!=='UNIFORM';
        if (profile && !['UNIFORM','WEDGE','TAPER','QUARTER_TAPER','EYE','MIRRORED_TAPER','CUSTOM'].includes(profile.widthProfile)) reject('Unsupported width profile.');
        if (profile?.widthProfile==='CUSTOM') {
          const points=profile.variableWidthPoints;
          if (!Array.isArray(points) || points.length<2 || points.length>256 || points.some((p,i)=>!Number.isFinite(p.position)||p.position<0||p.position>1||!Number.isFinite(p.width)||p.width<0||p.width>1e7||(i>0&&p.position<=points[i-1].position))) reject('Invalid custom width profile.');
        }
        if (nonuniform) strokeNote=`Width profile: ${profile.widthProfile}${profile.widthProfile==='CUSTOM' ? ' '+JSON.stringify(profile.variableWidthPoints) : ''}`;
        if (complex && complex.type!=='BASIC') {
          if (complex.type==='BRUSH' && complex.brushType==='STRETCH' && ['HEIST','BLOCKBUSTER'].includes(complex.brushName)) {
            if (!['FORWARD','BACKWARD'].includes(complex.direction)) reject('Unsupported brush direction.');
            strokeNote=(strokeNote ? strokeNote+'; ' : '')+`Stretch brush: ${complex.brushName}, ${complex.direction}`;
          } else if (complex.type==='DYNAMIC') {
            if (nonuniform) reject('Dynamic strokes cannot combine with variable width profiles.');
            if (!Number.isFinite(complex.frequency)||complex.frequency<Math.fround(.01)||complex.frequency>20||!Number.isFinite(complex.wiggle)||complex.wiggle<0||complex.wiggle>1e7||!Number.isFinite(complex.smoothen)||complex.smoothen<0||complex.smoothen>1) reject('Invalid dynamic stroke parameters.');
            strokeNote=`Dynamic stroke: frequency ${number(complex.frequency)}, wiggle ${number(complex.wiggle)}, smoothen ${number(complex.smoothen)}`;
          } else reject('Unsupported brush type/name. Only HEIST and BLOCKBUSTER stretch brushes and resolved dynamic strokes are supported.');
        }
        styledStroke=!!nonuniform || !!(complex && complex.type!=='BASIC');
        if (styledStroke) {
          resolved=true;
          if (!['VECTOR','LINE','RECTANGLE','ELLIPSE','POLYGON','STAR','BOOLEAN_OPERATION'].includes(n.type)) reject('Styled strokes require shape geometry.');
          if (!Number.isFinite(n.strokeWeight)||n.strokeWeight<0) reject('Invalid styled stroke width.');
          if (n.strokeWeight>0 && n.strokeAlign!=='CENTER' && (n.type==='LINE' || !n.fillGeometry?.length || n.vectorPaths?.some(p=>p.windingRule==='NONE'))) reject('Aligned styled strokes require a closed fill domain; open/mixed networks are unsupported.');
          if (n.strokeWeight>0 && (!Array.isArray(n.strokeGeometry)||!n.strokeGeometry.some(p=>typeof p.data==='string' && /[LQC]/.test(p.data)))) reject('Styled stroke outlines are unavailable; no constant-width fallback.');
        }
      }
      if (strokes.length > 32) reject('Stroke paint stack exceeds 32 paints.');
      if (boolean && strokes.length && (typeof n.strokeWeight !== 'number' || !Number.isFinite(n.strokeWeight) || n.strokeWeight < 0)) reject('Invalid boolean stroke width.');
      if(strokes.length && n.dashPattern?.length) {
        if(!Array.isArray(n.dashPattern) || n.dashPattern.length>32 || n.dashPattern.some(v=>typeof v!=='number' || !Number.isFinite(v) || v<0 || v>1e7)) reject('Invalid dash pattern.');
        if(effects.length && !resolved) reject('Dashed stroke effects are unsupported.');
        if(!resolved && 'strokeTopWeight' in n && [n.strokeRightWeight,n.strokeBottomWeight,n.strokeLeftWeight].some(v=>v!==n.strokeTopWeight)) reject('Dashed individual borders require uniform widths.');
      }
      const arc = n.type === 'ELLIPSE' && n.arcData && (n.arcData.startingAngle !== 0 || Math.abs(n.arcData.endingAngle - Math.PI*2) > 1e-6 || n.arcData.innerRadius !== 0);
      const primitive = !resolved && (arc || ['POLYGON','STAR'].includes(n.type));
      if (strokes.length && !resolved && !primitive && n.type !== 'VECTOR' && n.type !== 'LINE' && !n.dashPattern?.length && n.strokeJoin && n.strokeJoin !== 'MITER') reject(`Unsupported stroke join: ${String(n.strokeJoin)}.`);
      for (const e of effects) {
        if (Object.keys(e.boundVariables || {}).length && !verifiedBindings.has(n)) reject('Effect variable bindings must be resolved before export.');
        if (!['DROP_SHADOW', 'INNER_SHADOW'].includes(e.type)) reject(`Unsupported effect: ${e.type}.`);
        if (e.blendMode && e.blendMode !== 'NORMAL') reject(`Unsupported effect blend mode: ${e.blendMode}.`);
      }
      const radii = 'topLeftRadius' in n ? [n.topLeftRadius, n.topRightRadius, n.bottomRightRadius, n.bottomLeftRadius] : [0,0,0,0];
      radii.forEach(number);
      if(scope && allEffects.some(e=>e.type==='BACKGROUND_BLUR')) {
        if(['GROUP','TEXT','LINE'].includes(n.type))reject('Background blur requires an explicit filled shape or frame boundary.');
        scope.hasBackground=fills.some(p=>(p.opacity??1)>0);
        scope.boundary=(!smoothed && ['RECTANGLE','FRAME','COMPONENT','INSTANCE','COMPONENT_SET'].includes(n.type))
          ? `Clip::rounded_rect(${rect(box)}, ${array(radii)})`
          : `Clip::paths(${vectorCode(n)})`;
      }
      if (primitive) {
        if (effects.length) reject('Parameterized primitive effects are unsupported.');
        const radius = n.cornerRadius ?? 0;
        if (typeof radius !== 'number' || !Number.isFinite(radius) || radius < 0 || radius > 1e7) reject('Invalid primitive corner radius.');
        if (arc) {
          const a = n.arcData;
          if (![a.startingAngle,a.endingAngle,a.innerRadius].every(Number.isFinite) || Math.abs(a.startingAngle)>1e7 || Math.abs(a.endingAngle)>1e7 || Math.abs(a.endingAngle-a.startingAngle)>Math.PI*2+1e-5 || a.innerRadius<0 || a.innerRadius>1) reject('Invalid ellipse arc parameters.');
          if (radius) reject('Rounded ellipse arc corners are unsupported.');
          if (a.innerRadius===1 && strokes.length && n.strokeAlign==='INSIDE' && n.width>0 && n.height>0 && n.strokeWeight/2 >= (Math.min(n.width,n.height)/2)**2/(Math.max(n.width,n.height)/2)) reject('Inside arc stroke exceeds ellipse curvature radius.');
        } else {
          if (!Number.isInteger(n.pointCount) || n.pointCount<3 || n.pointCount>256) reject('Primitive point count must be 3 to 256.');
          if (n.type==='STAR' && (!Number.isFinite(n.innerRadius) || n.innerRadius<0 || n.innerRadius>1)) reject('Star inner radius must be 0 to 1.');
        }
      }
      let expression = null;
      if (n.type === 'TEXT') {
        if (strokes.length) reject('Text strokes are unsupported.');
        if (n.paragraphSpacing || n.paragraphIndent || n.hangingPunctuation || n.hangingList) reject('Paragraph spacing, indentation, and hanging text are unsupported.');
        if (n.textTruncation && n.textTruncation !== 'DISABLED' || n.maxLines != null) reject('Text truncation and maximum line counts are unsupported.');
        if (n.hasMissingFont) reject('The text uses a missing font.');
        if (!['LEFT','CENTER','RIGHT'].includes(n.textAlignHorizontal) || !['TOP','CENTER','BOTTOM'].includes(n.textAlignVertical)) reject('Unsupported text alignment.');
        const text = n.characters;
        if (typeof text !== 'string') reject('Missing text content.');
        else {
          if (unescape(encodeURIComponent(text)).length > 512) reject('Text exceeds the 512-byte limit.');
          if ([...text].some(c => { const p = c.codePointAt(0); return c !== '\n' && !(p>=0x20&&p<=0x7e || p>=0xa0&&p<=0xac || p>=0xae&&p<=0x24f || p>=0x2010&&p<=0x2027 || p>=0x2030&&p<=0x205e || p>=0x20a0&&p<=0x20bf || p>=0x2100&&p<=0x22ff); })) reject('Text contains characters outside figleaf’s Latin text scope.');
        }
        if(segments) {
          if(segments.length>128)reject('Text exceeds 128 styled runs.');
          let end=0;
          for(const s of segments) {
            if(s.start!==end || !Number.isInteger(s.end) || s.end<=s.start || s.characters!==text?.slice(s.start,s.end))reject('Invalid or incomplete styled text ranges.');
            end=s.end;
          }
          if(end!==text?.length)reject('Styled text ranges do not cover the text.');
        }
        const styles=segments?.length?segments:[n];
        const generated=[];
        for(const s of styles) {
          const paints=visible(s.fills||[]);
          if(paints.length!==1||paints[0]?.type!=='SOLID')reject('Text requires exactly one solid fill per run.');
          if(mixedRuns&&paints.some(blended))reject('Blended paints inside styled text runs are unsupported.');
          for(const p of paints) if(p.type==='SOLID' && (!p.color || [p.color.r,p.color.g,p.color.b,p.color.a??1,p.opacity??1].some(v=>typeof v!=='number'||!Number.isFinite(v)||v<0||v>1)))reject('Invalid text run color or opacity.');
          if(!s.fontName||typeof s.fontName!=='object'||typeof s.fontName.family!=='string'||typeof s.fontName.style!=='string'||typeof s.fontSize!=='number'||typeof s.letterSpacing!=='object'||typeof s.lineHeight!=='object')reject('Missing or invalid run text style.');
          if(s.textCase!=='ORIGINAL'||s.textDecoration!=='NONE')reject('Text case transformations and decorations are unsupported.');
          if(s.listOptions?.type!==undefined&&s.listOptions.type!=='NONE'||s.indentation||s.paragraphSpacing||s.paragraphIndent||Object.keys(s.openTypeFeatures||{}).length)reject('Lists, indentation, paragraph spacing, and OpenType overrides are unsupported.');
          if(Object.keys(s.boundVariables||{}).length&&!verifiedBindings.has(n))reject('Text run variable bindings must be resolved before export.');
          if(!['PIXELS','PERCENT','AUTO'].includes(s.lineHeight?.unit))reject('Unknown text line height unit.');
          if(!['PIXELS','PERCENT'].includes(s.letterSpacing?.unit))reject('Unsupported letter spacing unit.');
          const height=s.lineHeight?.unit==='AUTO'?s.fontSize:s.lineHeight?.unit==='PERCENT'?s.fontSize*s.lineHeight.value/100:s.lineHeight?.value;
          const tracking=s.letterSpacing?.unit==='PERCENT'?s.fontSize*s.letterSpacing.value/100:s.letterSpacing?.value;
          if(!(s.fontSize>0&&s.fontSize<=128&&height>=s.fontSize&&height<=256&&Math.abs(tracking)<=s.fontSize&&n.width>0&&n.width<=2048))reject('Text exceeds figleaf’s size, line-height, tracking, or width limits.');
          if(errors.length===before) {
            const key=JSON.stringify(s.fontName);let font=fonts.findIndex(f=>JSON.stringify(f)===key);
            if(font<0){font=fonts.length;fonts.push(s.fontName);}if(fonts.length>64)reject('More than 64 fonts are unsupported.');
            generated.push(`${mixedRuns?'figleaf::text::TextRun':'Text'}::new(${string(mixedRuns?s.characters:text)}${mixedRuns?'':`, ${rect(box)}`}).font(fonts[${font}]).size(${number(s.fontSize)}, ${number(height)})${s.lineHeight?.unit==='AUTO'?'.auto_line_height()':''}.letter_spacing(${number(tracking)}).color(${color(paints[0].color,paints[0].opacity)})`);
          }
        }
        for (const type of ['DROP_SHADOW','INNER_SHADOW']) {
          if (effects.filter(e => e.type === type).length > 1) reject(`Text allows only one ${type}.`);
        }
        if (effects.some(e => e.spread || e.showShadowBehindNode)) reject('Text shadow spread and show-behind are unsupported.');
        if (errors.length === before && text) {
          const align = { LEFT:'Start', TOP:'Start', CENTER:'Center', RIGHT:'End', BOTTOM:'End' };
          expression=mixedRuns?`Text::rich(vec![${generated.join(', ')}], ${rect(box)})`:generated[0];
          expression += `.align(figleaf::paint::Align::${align[n.textAlignHorizontal]}, figleaf::paint::Align::${align[n.textAlignVertical]}).wrap()`;
          for (const e of effects) expression += `.${e.type === 'DROP_SHADOW' ? 'drop_shadow' : 'inner_shadow'}(${color(e.color)}, ${array([e.offset.x,e.offset.y])}, ${number(e.radius)})`;
        }
      } else if (n.type === 'LINE' && !resolved) {
        if (fills.length || !strokes.length || effects.length) reject('Lines require stroke paints, no fills, and no effects.');
        if (errors.length === before) expression = `Shape::vector(${rect(box)}, vec![VectorPath::new(WindingRule::None).move_to([0.0, 0.0]).line_to(${array([n.width,0])})]).stroke(${number(n.strokeWeight)}, ${strokeColor(strokes[0])}, StrokeAlign::Center)`;
      } else if (n.type !== 'GROUP' && types.includes(n.type)) {
        if (n.type === 'VECTOR' || resolved) {
          expression = `Shape::vector(${rect(box)}, ${vectorCode(n)})`;
        } else if (primitive) {
          expression = arc ? `Shape::ellipse_arc(${rect(box)}, ArcData::new(${number(n.arcData.startingAngle)}, ${number(n.arcData.endingAngle)}, ${number(n.arcData.innerRadius)}))`
            : n.type==='POLYGON' ? `Shape::polygon(${rect(box)}, ${n.pointCount})`
            : `Shape::star(${rect(box)}, ${n.pointCount}, ${number(n.innerRadius)})`;
          if (n.cornerRadius) expression += `.radius(${number(n.cornerRadius)})`;
        } else {
          expression = `Shape::${n.type === 'ELLIPSE' ? 'ellipse' : 'rect'}(${rect(box)})`;
        }
        if (!primitive && !resolved && radii.some(r => r)) expression += `.radii(${array(radii)})`;
        // Figma's paint arrays are in drawing order, bottom to top.
        for (const p of fills) {
          if (p.type==='IMAGE' || p.type==='PATTERN') expression += `.paint(${paintCode(p)})`;
          if (p.type === 'SOLID') expression += `.fill(${color(p.color,p.opacity)})`;
          if (p.type.startsWith('GRADIENT_') && p.gradientTransform) expression += `.gradient(${gradientCode(p)})`;
          if(blended(p))expression+=`.fill_blend(BlendMode::${blendModes[p.blendMode]})`;
        }
        if (strokes.length) {
          const p = strokes[0];
          const align = { INSIDE:'Inside', CENTER:'Center', OUTSIDE:'Outside' }[n.strokeAlign];
          if (!align) reject('Unsupported stroke alignment.');
          const weight='strokeTopWeight' in n ? Math.max(n.strokeTopWeight,n.strokeRightWeight,n.strokeBottomWeight,n.strokeLeftWeight) : n.strokeWeight;
          if (resolved && (!Number.isFinite(weight) || weight < 0 || ('strokeTopWeight' in n && [n.strokeTopWeight,n.strokeRightWeight,n.strokeBottomWeight,n.strokeLeftWeight].some(w=>!Number.isFinite(w)||w<0)))) reject('Invalid resolved border width.');
          if ('strokeTopWeight' in n && n.type !== 'VECTOR' && !primitive && !boolean && (!resolved || weight > 0)) expression += `.strokes(${array([n.strokeTopWeight,n.strokeRightWeight,n.strokeBottomWeight,n.strokeLeftWeight])}, ${strokeColor(p)}, StrokeAlign::${align})`;
          else expression += `.stroke(${number(n.strokeWeight)}, ${strokeColor(p)}, StrokeAlign::${align})`;
          if (resolved && weight > 0) expression += `.stroke_geometry(${vectorCode(n, true)})`;
        }
        for (const e of effects) if (['DROP_SHADOW','INNER_SHADOW'].includes(e.type)) {
          expression += `.shadow(Shadow::${e.type === 'INNER_SHADOW' ? 'inner' : 'drop'}(${color(e.color)}, ${array([e.offset.x,e.offset.y])}, ${number(e.radius)}).spread(${number(e.spread ?? 0)})${e.showShadowBehindNode ? '.show_behind()' : ''})`;
        }
      }
      if (expression && strokes.length && (strokes.length>1 || strokes[0].type!=='SOLID')) expression += `.stroke_paints(vec![${strokes.map(paintCode).join(', ')}])`;
      if(expression && strokes.some(blended))expression+=`.stroke_blend_modes(vec![${strokes.map(p=>`BlendMode::${blendModes[p.blendMode ?? 'NORMAL']}`).join(', ')}])`;
      if (strokes.length && !resolved && (primitive || ['VECTOR','LINE'].includes(n.type) || n.dashPattern?.length)) {
        const strokeWidth='strokeTopWeight' in n && n.type!=='VECTOR' && !primitive ? n.strokeTopWeight : n.strokeWeight;
        if(typeof strokeWidth!=='number' || strokeWidth<0 || !Number.isFinite(strokeWidth)) reject('Invalid stroke width.');
        const cap=caps[n.strokeCap] || (n.type==='VECTOR' && (typeof n.strokeCap==='symbol' || n.strokeCap===undefined) ? 'Butt' : undefined) || (!['VECTOR','LINE'].includes(n.type) && n.strokeCap===undefined ? 'Butt' : undefined);
        const join={MITER:'Miter',BEVEL:'Bevel',ROUND:'Round'}[n.strokeJoin];
        if(!cap || !join) reject('Unsupported or mixed stroke caps/joins.');
        const miter=n.strokeMiterLimit ?? (!['VECTOR','LINE'].includes(n.type) ? 4 : undefined);
        if(typeof miter!=='number' || !Number.isFinite(miter) || miter<1) reject('Invalid stroke miter limit.');
        if(expression && cap && join && miter>=1) expression+=`.stroke_cap(StrokeCap::${cap}).stroke_join(StrokeJoin::${join}).stroke_miter_limit(${number(miter)})`;
        if(n.type==='VECTOR') expression+=`.stroke_paths(${strokeCode(n)})`;
        if(n.dashPattern?.length) expression+=`.dash_pattern(vec!${array(n.dashPattern)})`;
      }
      if (expression) expression += `.transform(${matrix(world)})`;
      if (expression) for (const c of clip || []) expression += `.intersect_clip(${c.paths ? `Clip::paths(${c.paths})` : `Clip::rounded_rect(${rect(c.rect)}, ${array(c.radii)})`}.transform(${matrix(c.transform)}))`;
      if (expression && errors.length === before) {
        for (const note of verifiedBindings.get(n) || []) lines.push(`    // Resolved binding: ${comment(note)}`);
        if (strokeNote) lines.push(`    // ${comment(strokeNote)} (resolved Figma outlines).`);
        if (smoothed) lines.push(`    // Corner smoothing: ${number(smoothing)} (resolved Figma contours).`);
        if(n.type==='TEXT' && fills.some(blended)) {const name=`composite_${compositeIndex++}`;lines.push(`    let mut ${name} = Canvas::new();`,`    ${name}.text(${expression});`,`    ${target}.composite(Composite::new(${name}).blend_mode(BlendMode::${blendModes[fills[0].blendMode]}));`);}
        else lines.push(`    // ${comment(path)} [${comment(n.id)}]`, `    ${target}.${n.type === 'TEXT' ? 'text' : 'shape'}(${expression});`);
      }
      if (n.clipsContent) {
        if (radii.some(r=>r<0)) reject('Invalid clip corner radius.');
        childClip = [...(clip || []), { rect: box, radii, transform: world, paths: smoothed ? vectorCode(n) : null }];
        if (childClip.length>128) reject('Clip stack exceeds 128 clips.');
      }
    } catch (e) { reject(e.message); }
    // Boolean operands are consumed by Figma’s resolved paths, not painted again.
    if (n.type === 'BOOLEAN_OPERATION') for (const child of n.children || []) operands(child,path+' / '+child.name,depth+1);
    let children = n.type === 'BOOLEAN_OPERATION' ? [] : n.children || [];
    if (n.itemReverseZIndex) children = [...children].reverse();
    let active = null;
    const finishMask = () => {
      if (!active) return;
      const {id,kind,opacity} = active;
      lines.push(`    ${target}.masked(Mask::new(MaskType::${kind}, mask_${id}).opacity(${number(opacity)}), masked_${id});`);
      active = null;
    };
    for (const child of children) {
      if (child.visible === false) continue;
      if (child.isMask || child.clipsContent) finishMask();
      if (child.isMask) {
        const id=maskIndex++, kind={VECTOR:'Vector',ALPHA:'Alpha',LUMINANCE:'Luminance'}[child.maskType];
        const opacity=child.opacity ?? 1;
        if (!Number.isFinite(opacity) || opacity<0 || opacity>1) fail(child,path+' / '+child.name,'Invalid mask opacity.');
        lines.push(`    let mut mask_${id} = Canvas::new();`);
        visit(child,path+' / '+child.name,world,childClip,false,depth+1,`mask_${id}`,true,maskDepth+1,compositeDepth);
        lines.push(`    let mut masked_${id} = Canvas::new();`);
        active={id,kind,opacity};
      } else visit(child,path+' / '+child.name,world,childClip,false,depth+1,active ? `masked_${active.id}` : target,false,maskDepth+(active?1:0),compositeDepth);
    }
    finishMask();
    if(scope){try {
      let expression=`Composite::new(${scope.name}).opacity(${number(asMask?1:(n.opacity??1))}).blend_mode(BlendMode::${scope.mode}).effect_transform(${matrix(world)})`;
      for(const c of scope.clips)expression+=`.intersect_clip(${c.paths ? `Clip::paths(${c.paths})` : `Clip::rounded_rect(${rect(c.rect)}, ${array(c.radii)})`}.transform(${matrix(c.transform)}))`;
      const shadows=scope.effects.filter(e=>['DROP_SHADOW','INNER_SHADOW'].includes(e.type));
      for(const e of shadows)expression+=`.shadow(${shadowCode(e)})`;
      if(shadows.some(blended))expression+=`.shadow_blend_modes(vec![${shadows.map(e=>`BlendMode::${blendModes[e.blendMode??'NORMAL']}`).join(', ')}])`;
      for(const e of scope.effects) {
        if(e.type==='LAYER_BLUR')expression+=`.layer_blur(${number(e.radius)})`;
        if(e.type==='BACKGROUND_BLUR'&&scope.hasBackground)expression+=`.background_blur(${number(e.radius)}, ${scope.boundary}.transform(${matrix(world)}))`;
      }
      lines.push(`    ${outerTarget}.composite(${expression});`);
    }catch(e){reject(e.message);}}
  }
  const compositeRoot=needsComposite(selection[0]);
  if(compositeRoot)lines.push('    let mut selection_content = Canvas::new();');
  try { visit(selection[0], selection[0].name, identity, null, true,0,compositeRoot?'selection_content':'canvas');
    if(compositeRoot)lines.push('    canvas.composite(Composite::new(selection_content));');
  }
  catch (e) { fail(selection[0], selection[0].name, e.message); }
  if(compositeIndex>128)fail(selection[0],selection[0].name,'Hierarchy exceeds 128 composite scopes.');
  if (maskIndex > 32) fail(selection[0],selection[0].name,'Hierarchy exceeds 32 mask scopes.');
  if (errors.length) return { code: null, errors };
  const preamble = [
    '// Generated by figleaf. Current dimensions and resolved values; no responsive layout or interactions.',
    '// Root placement is omitted; its rotation/scale and all descendant transforms are preserved.',
    ...(compositeRoot?['// The selection is isolated: backdrop-dependent blends use only its included content.']:[]),
    '// Supply matching fonts/styles through your text backend. Render at scale 0.5–4.',
    ...fonts.map((f,i) => `// fonts[${i}]: ${comment(f.family)} / ${comment(f.style)}`),
    '#[allow(unused_imports)]',
    'use figleaf::{Canvas, Rect, Shape, Text, Shadow, StrokeAlign, Gradient, LinearGradient, VectorPath, WindingRule, Transform, StrokeCap, StrokeJoin, ArcData, Clip, Mask, MaskType, Paint, Image, ImagePaint, ImageScaleMode, Pattern, TileType, PatternAlignment, Composite, BlendMode};',
    '', `pub fn draw_selection(canvas: &mut Canvas${fonts.length ? `, fonts: [usize; ${fonts.length}]` : ''}) {`,
  ];
  return { code: [...preamble, ...lines, '}',''].join('\n'), errors: [], fonts, nodeCount: count };
}
if (typeof module !== 'undefined') module.exports = { exportHierarchy };
