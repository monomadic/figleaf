const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { exportHierarchy } = require('./exporter');
const solid = { type: 'SOLID', color: { r: 1, g: 0, b: 0 }, opacity: 1, blendMode: 'NORMAL' };
function node(props = {}) {
  return { id: '1:1', name: 'Rectangle', type: 'RECTANGLE', visible: true, opacity: 1, blendMode: 'NORMAL', relativeTransform: [[1,0,0],[0,1,0]], width: 100, height: 40, fills: [solid], strokes: [], effects: [], cornerSmoothing: 0, topLeftRadius: 0, topRightRadius: 0, bottomLeftRadius: 0, bottomRightRadius: 0, strokeTopWeight: 0, strokeRightWeight: 0, strokeBottomWeight: 0, strokeLeftWeight: 0, strokeAlign: 'INSIDE', strokeJoin: 'MITER', strokeCap:'NONE', strokeMiterLimit:4, ...props };
}
function text(props = {}) {
  return node({ type:'TEXT', name:'Label', characters:'Hello "Rust" \\ world', fontName:{family:'Inter',style:'Regular'}, fontSize:12, lineHeight:{unit:'PIXELS',value:16}, letterSpacing:{unit:'PIXELS',value:0}, textCase:'ORIGINAL', textDecoration:'NONE', textAlignHorizontal:'LEFT', textAlignVertical:'CENTER', ...props });
}
function fixture() {
  return node({ type:'FRAME', name:'Frame', relativeTransform:[[1,0,500],[0,1,200]], clipsContent:true, fills:[], strokes:[solid], strokeBottomWeight:1, children:[
    node({name:'Pill', relativeTransform:[[-1,0.25,33],[0,1.5,4]], width:30, height:20, topLeftRadius:8,topRightRadius:8,bottomLeftRadius:8,bottomRightRadius:8, fills:[solid,{type:'GRADIENT_LINEAR', opacity:0.5, gradientTransform:[[1,0,0],[0,1,0]], gradientStops:[{position:0,color:{r:0,g:0,b:1,a:1}},{position:1,color:{r:0,g:1,b:0,a:0}}]}], effects:[{type:'DROP_SHADOW',color:{r:0,g:0,b:0,a:0.2},offset:{x:0,y:2},radius:4,spread:0}]}),
    text({relativeTransform:[[1,0,35],[0,1,4]],width:60,height:20}),
    node({type:'VECTOR',name:'Curved path',fillGeometry:[{windingRule:'EVENODD',data:'M0 0 Q5 -5 10 0 C15 5 15 15 10 20 L0 20 Z M2 4 L8 4 L8 10 L2 10 Z'}],vectorPaths:[{windingRule:'NONE',data:'M10 20 L15 25 L20 20'}]}),
    node({name:'Dot',type:'ELLIPSE',width:8,height:8,relativeTransform:[[1,0,90],[0,1,30]]}),
    node({type:'LINE',name:'Line',width:10,height:0,fills:[],strokes:[solid],strokeWeight:2,strokeCap:'ROUND',relativeTransform:[[0,-1,15],[1,0,15]]}),
  ] });
}
test('deterministic complete hierarchy and Rust compilation', () => {
  const result = exportHierarchy([fixture()]);
  assert.deepEqual(result.errors, []);
  assert.equal(result.code, exportHierarchy([fixture()]).code);
  assert.match(result.code, /strokes\(\[0.0, 0.0, 1.0, 0.0\]/);
  assert.match(result.code, /Rect::new\(0.0, 0.0, 30.0, 20.0\)/);
  assert.match(result.code, /fonts\[0\]: Inter \/ Regular/);
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'figleaf-plugin-test-'));
  try {
    const root = path.resolve(__dirname, '..');
    execFileSync('rustc', ['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.join(root,'src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'generated.rs'), result.code + '\nfn main() { let mut c = Canvas::new(); draw_selection(&mut c, [0]); assert_eq!(c.items().len(), 6); for item in c.items() { if let figleaf::paint::Item::Shape(s) = item { assert_eq!(s.problem(), None); } } }');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'generated.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'generated')]);
    execFileSync(path.join(tmp,'generated'));
  } finally { fs.rmSync(tmp,{recursive:true,force:true}); }
});
test('all-or-nothing errors include every layer', () => {
  const result = exportHierarchy([node({type:'FRAME',children:[node({name:'Icon',type:'VECTOR'}),text({name:'Title',fontSize:Symbol('mixed')}),node({name:'Blur',effects:[{type:'LAYER_BLUR'}]})]})]);
  assert.equal(result.code,null);
  for (const name of ['Icon','Title','Blur']) assert.ok(result.errors.some(e => e.path.endsWith(name)));
});
test('unsupported features never produce partial Rust', () => {
  const cases = [
    node({opacity:0.5}), node({isMask:true}), node({cornerSmoothing:0.6}), node({blendMode:'MULTIPLY'}),
    node({fills:[{type:'IMAGE'}]}), node({fills:[{...solid,blendMode:'SCREEN'}]}),
    node({relativeTransform:[[0,0,0],[1,0,0]]}),
    node({strokes:[solid,solid]}), node({strokes:[solid],dashPattern:[-1,2]}),
    node({strokes:[solid],strokeJoin:'BEVEL'}),
    node({fills:[{type:'GRADIENT_RADIAL'}]}),
    node({fills:[{type:'GRADIENT_LINEAR',gradientTransform:[[1,0,0],[0,1,0]],gradientStops:Array(9).fill({position:0,color:{r:0,g:0,b:0,a:1}})}]}),
    node({type:'ELLIPSE',arcData:{startingAngle:0,endingAngle:1,innerRadius:2}}),
    text({lineHeight:{unit:'AUTO'}}),text({textDecoration:'UNDERLINE'}),text({characters:'🐈'}),
    text({getStyledTextSegments:() => [{},{ }]}),text({paragraphSpacing:4}),text({textTruncation:'ENDING'}),
    text({getStyledTextSegments:() => [{openTypeFeatures:{liga:0}}]}),
    text({fills:[solid,solid]}),text({characters:'a'.repeat(513)}), text({fontSize:129}),
  ];
  for (const n of cases) { const r = exportHierarchy([n]); assert.equal(r.code,null,JSON.stringify(n)); assert.ok(r.errors.length); }
});
test('ancestor clips are retained for shapes and text', () => {
  const r = exportHierarchy([node({type:'FRAME',clipsContent:true,width:50,height:50,children:[node({type:'FRAME',width:50,height:50,relativeTransform:[[1,0,20],[0,1,20]],clipsContent:true,children:[node()]})]})]);
  assert.deepEqual(r.errors,[]); assert.equal((r.code.match(/intersect_clip\(/g)||[]).length,3);
  const round = node({type:'FRAME',clipsContent:true,topLeftRadius:8,topRightRadius:8,bottomLeftRadius:8,bottomRightRadius:8,children:[text()]});
  assert.deepEqual(exportHierarchy([round]).errors,[]);
  round.children=[node({...round,children:[node()],relativeTransform:[[1,0,20],[0,1,20]]})];
  assert.deepEqual(exportHierarchy([round]).errors,[]);
});
test('selection validation, hidden subtrees, and hostile names', () => {
  assert.equal(exportHierarchy([]).code,null); assert.equal(exportHierarchy([node(),node()]).code,null);
  const r = exportHierarchy([node({name:'x\n}\npanic!(); //', children:[node({visible:false,type:'VECTOR',children:[node({type:'IMAGE'})]})]})]);
  assert.deepEqual(r.errors,[]); assert.match(r.code,/\/\/ x } panic!\(\); \/\//);
});
test('plugin lifecycle is read-only, page-local, and invalidates results', async () => {
  const events = {}, messages = [], pageEvents = {};
  const page = {selection:[fixture()],on:(key,fn)=>pageEvents[key]=fn,off:()=>{}};
  const figma = {root:{documentColorProfile:'SRGB'},currentPage:page,showUI:()=>{},on:(key,fn)=>events[key]=fn,ui:{postMessage:m=>messages.push(m)}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,'dist/code.js'),'utf8'),{figma});
  assert.equal(messages.at(-1).type,'invalidated');
  await figma.ui.onmessage({type:'generate'}); assert.equal(messages.at(-1).errors.length,0);
  pageEvents.nodechange(); assert.equal(messages.at(-1).type,'invalidated');
  page.selection=[]; events.selectionchange(); await figma.ui.onmessage({type:'generate'}); assert.equal(messages.at(-1).code,null);
  assert.equal(events.documentchange,undefined);
});
test('unchecked variable bindings cannot bypass resolution', () => {
  for (const n of [node({boundVariables:{opacity:{id:'v'}}}),node({fills:[{...solid,boundVariables:{color:{id:'v'}}}]}),node({effects:[{type:'DROP_SHADOW',boundVariables:{radius:{id:'v'}},color:{r:0,g:0,b:0,a:1},offset:{x:0,y:0},radius:2}]})]) {
    assert.equal(exportHierarchy([n]).code,null);
  }
});
test('UI clears stale code, renders literal errors, and only enables valid output', async () => {
  const elements = new Map();
  const element = () => ({value:'',textContent:'',hidden:false,disabled:true,children:[],append(...children){this.children.push(...children);},replaceChildren(){this.children=[];},focus(){},select(){}});
  const document = {getElementById:id=>{if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element};
  const messages = [], window = {};
  const html = fs.readFileSync(path.join(__dirname,'ui.html'),'utf8');
  vm.runInNewContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], {document,window,parent:{postMessage:m=>messages.push(m)},navigator:{clipboard:{writeText:async()=>{}}}});
  assert.equal(messages[0].pluginMessage.type,'ready');
  const send = m => window.onmessage({data:{pluginMessage:m}});
  send({type:'invalidated',revision:1,selection:['Frame']});
  assert.equal(elements.get('generate').disabled,false);
  send({type:'result',revision:1,errors:[],code:'fn draw() {}',nodeCount:1});
  assert.equal(elements.get('copy').disabled,false);
  send({type:'invalidated',revision:2,selection:['Other']});
  assert.equal(elements.get('code').value,''); assert.equal(elements.get('copy').disabled,true);
  send({type:'result',revision:1,errors:[],code:'stale',nodeCount:1}); assert.equal(elements.get('code').value,'');
  send({type:'result',revision:2,errors:[{path:'<img onerror=bad()>',message:'Unsupported'}],code:null});
  assert.equal(elements.get('errors').hidden,false); assert.equal(elements.get('code').value,'');
  assert.equal(elements.get('error-list').children[0].children[0].textContent,'<img onerror=bad()>');
  send({type:'result',revision:2,errors:[],code:'fn draw() {}',nodeCount:1});
  await elements.get('copy').onclick(); assert.equal(elements.get('status').textContent,'Copied Rust code.');
});

test('wide-gamut documents and container effects fail closed', () => {
  assert.equal(exportHierarchy([node()], 'DISPLAY_P3').code, null);
  assert.equal(exportHierarchy([node({type:'FRAME',children:[node()],effects:[{type:'DROP_SHADOW',color:{r:0,g:0,b:0,a:1},offset:{x:0,y:0},radius:2}]})]).code,null);
});
test('resolved network regions, holes and unfilled branches survive export', () => {
  const geometry=[{windingRule:'NONZERO',data:'M0 0L20 0L20 20L0 20Z M5 5 L5 15 L15 15 L15 5 Z'}, {windingRule:'EVENODD',data:'M30 0 Q40 -10 50 0 C60 10 60 20 50 20 Z'}];
  const n=node({type:'VECTOR',fillGeometry:geometry,vectorPaths:[{windingRule:'NONE',data:'M20 10 L30 10 M30 10 L40 20 M30 10 L40 0'}],vectorNetwork:{regions:[{fills:[solid]}]}});
  const r=exportHierarchy([n]); assert.deepEqual(r.errors,[]);
  assert.match(r.code,/Shape::vector/); assert.match(r.code,/WindingRule::NonZero/);assert.match(r.code,/WindingRule::EvenOdd/);assert.match(r.code,/WindingRule::None/);
  assert.equal((r.code.match(/\.move_to\(/g)||[]).length,6);
  assert.match(r.code,/\.quadratic_to/);assert.match(r.code,/\.cubic_to/);
});
test('vector errors block output instead of dropping features or malformed commands', () => {
  const n=node({type:'VECTOR',fillGeometry:[{windingRule:'NONZERO',data:'M0 0 L10 0 L0 10 Z'}]});
  for(const overrides of [
    {strokes:[solid]}, {effects:[{type:'LAYER_BLUR',radius:2}]},
    {vectorNetwork:{regions:[{fills:[]}]}},
    ...['M0 0 A10 10 0 0 0 20 20','L0 0','M0 C1 2 3 4 5 6','M0 0 Q1 2','M0 0 LNaN 10','M0 0 L1e99 10'].map(data=>({fillGeometry:[{windingRule:'NONZERO',data}]})),
    {fillGeometry:[{windingRule:'UNKNOWN',data:'M0 0'}]},
    {fillGeometry:[{windingRule:'NONZERO',data:'M0 0 '.repeat(4097)}]},
  ]) { const r=exportHierarchy([{...n,...overrides}]);assert.equal(r.code,null,JSON.stringify(overrides));assert.ok(r.errors.length); }
});
test('Figma person icon fixture exports cubic contours and retains both holes', () => {
  const ref=require('./vector-reference.json');const c=ref.color;
  const n=node({type:'VECTOR',id:ref.id,width:ref.width,height:ref.height,fillGeometry:ref.fillGeometry,fills:[{type:'SOLID',color:{r:c.r,g:c.g,b:c.b},opacity:c.a}]});
  const r=exportHierarchy([n]);assert.deepEqual(r.errors,[]);
  assert.equal((r.code.match(/\.move_to\(/g)||[]).length,3);
  assert.equal((r.code.match(/\.cubic_to\(/g)||[]).length,10);
});

test('affine hierarchy composes local geometry and preserves root linear transform', () => {
  const r = exportHierarchy([node({type:'FRAME',fills:[],relativeTransform:[[0,-1,500],[1,0,200]], children:[
    node({relativeTransform:[[-2,0.5,10],[0,3,20]],width:20,height:10}),
    text({relativeTransform:[[1,0,4],[0,1,8]]})
  ]})]);
  assert.deepEqual(r.errors,[]);
  assert.match(r.code,/Transform::new\(\[\[0.0, -3.0, -20.0\], \[-2.0, 0.5, 10.0\]\]\)/);
  assert.match(r.code,/Transform::new\(\[\[0.0, -1.0, -8.0\], \[1.0, 0.0, 4.0\]\]\)/);
  const bad=exportHierarchy([node({relativeTransform:[[1,1,0],[1,1+1e-10,0]]})]);
  assert.equal(bad.code,null);
  assert.match(bad.errors[0].message,/singular/);
});

test('crossing transformed clips retain both transforms', () => {
  const r=exportHierarchy([node({type:'FRAME',clipsContent:true,children:[
    node({type:'FRAME',clipsContent:true,relativeTransform:[[1,0.5,0],[0,1,0]],children:[node()]})
  ]})]);
  assert.deepEqual(r.errors,[]);
  assert.equal((r.code.match(/intersect_clip\(/g)||[]).length,3);
  assert.match(r.code,/0.5/);
});

// Load the resolver with the same lexical exporter dependency as the bundle.
const resolverContext = { exportHierarchy, module: {exports:{}} };
vm.runInNewContext(fs.readFileSync(path.join(__dirname,'variables.js'),'utf8'), resolverContext);
const {exportWithVariables} = resolverContext.module.exports;
const alias = {type:'VARIABLE_ALIAS',id:'color'};
test('bindings resolve per consumer; literals and provenance are exported', async () => {
  const a=node({name:'Dark',fills:[{...solid,boundVariables:{color:alias}}]});
  const b=node({name:'Light',fills:[{...solid,color:{r:0,g:0,b:1},boundVariables:{color:alias}}]});
  const seen=[];
  const variables={getVariableByIdAsync:async id=>({name:'Theme / Accent',resolveForConsumer:n=>{
    seen.push(n.name); return {resolvedType:'COLOR',value:n.fills[0].color};
  }})};
  const root=node({type:'FRAME',fills:[],children:[a,b]});
  const result=await exportWithVariables([root],'SRGB',variables);
  assert.equal(result.errors.length,0);
  assert.match(result.code,/Theme \/ Accent \[color\]/);
  assert.match(result.code,/fill\(\[0.0, 0.0, 1.0, 1.0\]\)/);
  assert.deepEqual(seen,['Dark','Light']);
  assert.equal(a.fills[0].boundVariables.color,alias);
});
test('missing, malformed, unresolved and unsupported bound values fail closed', async () => {
  const n=node({boundVariables:{opacity:alias},opacity:0.5});
  for (const variable of [null,{resolveForConsumer:()=>({resolvedType:'COLOR',value:alias})},{resolveForConsumer:()=>{throw Error('cycle');}}]) {
    const r=await exportWithVariables([n],'SRGB',{getVariableByIdAsync:async()=>variable});
    assert.equal(r.code,null); assert.ok(r.errors.length);
  }
  const r=await exportWithVariables([n],'SRGB',{getVariableByIdAsync:async()=>({name:'Opacity',resolveForConsumer:()=>({resolvedType:'FLOAT',value:0.5})})});
  assert.equal(r.code,null); assert.ok(r.errors.some(e=>/compositing/.test(e.message)));
});

test('numeric, text, effect and gradient-stop bindings keep evaluated Figma properties', async () => {
  const bound = (id) => ({type:'VARIABLE_ALIAS',id});
  const n=fixture();
  n.boundVariables={width:bound('width')};
  n.children[0].effects[0].boundVariables={radius:bound('blur')};
  n.children[0].fills[1].gradientStops[0].boundVariables={color:bound('blue')};
  n.children[1].boundVariables={characters:[bound('label')]};
  const values={width:['FLOAT',100],blur:['FLOAT',4],blue:['COLOR',{r:0,g:0,b:1,a:1}],label:['STRING',n.children[1].characters]};
  const result=await exportWithVariables([n],'SRGB',{getVariableByIdAsync:async id=>({name:id,resolveForConsumer:()=>({resolvedType:values[id][0],value:values[id][1]})})});
  assert.equal(result.errors.length,0);
  for (const id of Object.keys(values)) assert.ok(result.code.includes(`[${id}]`));
  const explicit=exportHierarchy([fixture()]);
  assert.equal(result.code.replace(/^    \/\/ Resolved binding:.*\n/gm,''),explicit.code);
});

test('async variable lookup cannot publish after selection changes or a newer request', async () => {
  const events={}, messages=[], pending=[];
  const n=node({boundVariables:{width:alias}});
  const page={selection:[n],on:()=>{},off:()=>{}};
  const figma={root:{documentColorProfile:'SRGB'},currentPage:page,showUI:()=>{},on:(key,fn)=>events[key]=fn,
    variables:{getVariableByIdAsync:()=>new Promise(resolve=>pending.push(resolve))},ui:{postMessage:m=>messages.push(m)}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,'dist/code.js'),'utf8'),{figma});
  const variable={name:'Width',resolveForConsumer:()=>({resolvedType:'FLOAT',value:100})};
  const first=figma.ui.onmessage({type:'generate'});
  events.selectionchange(); pending.shift()(variable); await first;
  assert.equal(messages.at(-1).type,'invalidated');
  const older=figma.ui.onmessage({type:'generate'}), newer=figma.ui.onmessage({type:'generate'});
  pending.pop()(variable); await newer;
  const length=messages.length;
  pending.shift()(variable); await older;
  assert.equal(messages.length,length);
  assert.equal(messages.at(-1).type,'result');
});

test('hidden paints do not resolve mirrored node bindings', async () => {
  const n=node({boundVariables:{fills:[alias]},fills:[solid,{...solid,visible:false,boundVariables:{color:alias}}]});
  const result=await exportWithVariables([n],'SRGB',{getVariableByIdAsync:()=>{throw Error('must not look up hidden paint');}});
  assert.equal(result.errors.length,0);
});

test('real Figma stroke fixture exports centerlines, caps, joins and alignment and compiles', () => {
  const result=exportHierarchy([require('./stroke-reference.json')]);
  assert.deepEqual(result.errors,[]);
  for(const token of ['StrokeCap::Butt','StrokeCap::Square','StrokeCap::Round','StrokeJoin::Miter','StrokeJoin::Bevel','StrokeJoin::Round','StrokeAlign::Inside','StrokeAlign::Center','StrokeAlign::Outside','.cubic_to(','.stroke_paths(']) assert.ok(result.code.includes(token),token);
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-strokes-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),result.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);
    execFileSync(path.join(tmp,'main'));
  } finally {fs.rmSync(tmp,{recursive:true,force:true});}
});

test('unsupported stroke topology and styling fail closed', () => {
  const n=require('./stroke-reference.json').children[0];
  for(const patch of [
    {variableWidthStrokeProperties:{widthProfile:'WEDGE'}}, {complexStrokeProperties:{type:'BRUSH'}}, {strokeCap:'UNKNOWN_ARROW'}, {strokeJoin:'unknown'}, {strokeMiterLimit:0}, {strokeWeight:-1}, {dashPattern:[-2,2]},
    {vectorNetwork:{vertices:[{x:0,y:0},{x:10,y:0},{x:0,y:10},{x:10,y:10}],segments:[{start:0,end:1},{start:0,end:2},{start:0,end:3}]}},
    {vectorNetwork:{vertices:[{x:0,y:0}],segments:[]}},
    {vectorNetwork:{vertices:[{x:0,y:0}],segments:[{start:0,end:2}]}},
  ]) {const r=exportHierarchy([{...n,...patch}]);assert.equal(r.code,null);assert.ok(r.errors.length);}
});

test('Figma dash and endpoint fixture exports independent caps and compiles', () => {
  const result=exportHierarchy([require('./dash-reference.json')]);assert.deepEqual(result.errors,[]);
  for(const token of ['.dash_pattern(','.start_cap(StrokeCap::CircleFilled)','.end_cap(StrokeCap::ArrowEquilateral)','StrokeCap::ArrowLines','StrokeCap::TriangleFilled','StrokeCap::DiamondFilled']) assert.ok(result.code.includes(token),token);
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-dashes-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),result.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});

test('malformed dashes, unsupported decoration types and unequal borders fail closed',()=>{
  const n=require('./dash-reference.json').children[0];
  for(const patch of [{dashPattern:[-1,4]},{dashPattern:[NaN,4]},{dashPattern:Array(33).fill(1)},{strokeCap:'ERD_MANY'},{complexStrokeProperties:{type:'BRUSH'}}]) {
    const r=exportHierarchy([{...n,...patch}]);assert.equal(r.code,null);assert.ok(r.errors.length);
  }
  const rect=require('./dash-reference.json').children.at(-3);
  assert.equal(exportHierarchy([{...rect,strokeTopWeight:3}]).code,null);
});

test('Figma primitive fixture preserves inspector parameters and compiles',()=>{
  const result=exportHierarchy([require('./primitive-reference.json')]);assert.deepEqual(result.errors,[]);
  for(const token of ['Shape::ellipse_arc(', 'ArcData::new(', 'Shape::polygon(', 'Shape::star(', '.radius(100.0)']) assert.ok(result.code.includes(token),token);
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-primitives-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),result.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});
test('unsupported primitive geometry fails without partial code',()=>{
  const f=require('./primitive-reference.json');
  for(const [n,patch] of [[f.children[0],{cornerRadius:3}],[f.children[0],{arcData:{startingAngle:0,endingAngle:8,innerRadius:0}}],[f.children[3],{strokeWeight:100}],[f.children[6],{pointCount:2}],[f.children[6],{pointCount:3.5}],[f.children[9],{innerRadius:2}],[f.children[9],{cornerRadius:NaN}],[f.children[9],{effects:[{type:'DROP_SHADOW'}]}]]){
    const r=exportHierarchy([{...f,children:[{...n,...patch}]}]);assert.equal(r.code,null);assert.ok(r.errors.length);
  }
});

test('boolean fixture exports resolved contours once, preserves stroke outlines and compiles',()=>{
  const fixture=require('./boolean-reference.json');const result=exportHierarchy([fixture]);assert.deepEqual(result.errors,[]);
  assert.equal((result.code.match(/canvas.shape\(/g)||[]).length,7);
  assert.equal((result.code.match(/\.stroke_geometry\(/g)||[]).length,3);
  assert.ok(result.code.includes('StrokeAlign::Inside'));assert.ok(result.code.includes('StrokeAlign::Outside'));
  assert.ok(!result.code.includes('.stroke_paths('));assert.ok(!result.code.includes('.dash_pattern('));
  assert.ok(!result.code.includes('// Temporary figleaf boolean reference / Nested subtraction /'));
  assert.ok(result.nodeCount>7);
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-booleans-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),result.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});
test('boolean failures never fall back to operands or a rectangle',()=>{
  const n=require('./boolean-reference.json').children[1];
  for(const patch of [{booleanOperation:'UNKNOWN'},{strokeWeight:-1},{fillGeometry:undefined},{strokeGeometry:undefined},{strokeGeometry:[{windingRule:'NONE',data:'M0 0 L1 1'}]},{strokeGeometry:[{windingRule:'NONZERO',data:'M0 0 A1 1 0 0 0 1 1'}]},{effects:[{type:'LAYER_BLUR'}]},{strokes:[{...solid,type:'GRADIENT_LINEAR',gradientStops:[]}]}]) {
    const r=exportHierarchy([{...n,...patch}]);assert.equal(r.code,null);assert.ok(r.errors.length);
  }
  const empty=exportHierarchy([{...n,fillGeometry:[],strokeGeometry:[]}]);assert.deepEqual(empty.errors,[]);assert.ok(empty.code.includes('vec![]'));
});
test('boolean operand bindings are verified and retain provenance without painting operands',async()=>{
  const n=structuredClone(require('./boolean-reference.json').children[0]);n.children[0].boundVariables={width:alias};
  assert.equal(exportHierarchy([n]).code,null);
  const r=await exportWithVariables([n],'SRGB',{getVariableByIdAsync:async()=>({name:'Operand width',resolveForConsumer:()=>({resolvedType:'FLOAT',value:70})})});
  assert.deepEqual(r.errors,[]);assert.match(r.code,/Resolved operand binding/);assert.match(r.code,/Operand width/);
  assert.equal((r.code.match(/canvas.shape\(/g)||[]).length,1);
});

test('resolved boolean operands still obey hierarchy limits',()=>{
  const n=structuredClone(require('./boolean-reference.json').children[0]);
  let child=node();for(let i=0;i<130;i++)child=node({children:[child]});n.children=[child];
  const result=exportHierarchy([n]);assert.equal(result.code,null);assert.ok(result.errors.some(e=>/limit/.test(e.message)));
});

test('clipping fixture keeps every ancestor on shapes and text and compiles',()=>{
  const r=exportHierarchy([require('./clipping-reference.json')]);assert.deepEqual(r.errors,[]);
  assert.match(r.code,/Clip::rounded_rect\(Rect::new\(0.0, 0.0, 110.0, 100.0\), \[30.0, 0.0, 18.0, 8.0\]/);
  const rotated=r.code.split('\n').filter(l=>l.includes('canvas.text('))[1];assert.equal((rotated.match(/intersect_clip/g)||[]).length,2);
  assert.ok(!r.code.includes('.clip_transformed('));
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-clips-'));
  try{
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),r.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c,[0]);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
  const bad=structuredClone(require('./clipping-reference.json'));bad.children[0].topLeftRadius=-1;assert.equal(exportHierarchy([bad]).code,null);
});

test('smoothing exports resolved fills, borders, clips and shadows and compiles',()=>{
  const fixture=require('./smoothing-reference.json');
  const r=exportHierarchy([fixture]);assert.deepEqual(r.errors,[]);
  assert.match(r.code,/Corner smoothing: 0.6/);assert.match(r.code,/Clip::paths/);
  assert.match(r.code,/\.strokes\(\[0.0, 5.0, 3.0, 1.0\]/);
  assert.match(r.code,/Shadow::inner/);assert.match(r.code,/Shadow::drop/);
  assert.equal(r.code,fs.readFileSync(path.join(__dirname,'../examples/generated/smoothing.rs'),'utf8'));
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-smoothing-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),r.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for item in c.items(){if let figleaf::paint::Item::Shape(s)=item{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});
test('invalid smoothing and unavailable contours block all output',()=>{
  const n=require('./smoothing-reference.json').children[1];
  for(const patch of [{cornerSmoothing:NaN},{cornerSmoothing:-1},{cornerSmoothing:1.1},{fillGeometry:undefined},{strokeGeometry:undefined},{strokeTopWeight:-1},{effects:[{type:'LAYER_BLUR',radius:2}]}]) {
    const r=exportHierarchy([node(),{...n,...patch}]);assert.equal(r.code,null);assert.ok(r.errors.length);
  }
  const zero={...n,strokeWeight:0,strokeTopWeight:0,strokeRightWeight:0,strokeBottomWeight:0,strokeLeftWeight:0,strokeGeometry:undefined};
  assert.deepEqual(exportHierarchy([zero]).errors,[]);
});

test('mask fixture exports scoped canvases and compiles without dependencies',()=>{
  const result=exportHierarchy([require('./masks-reference.json')]);assert.deepEqual(result.errors,[]);
  for(const kind of ['Vector','Alpha','Luminance'])assert.match(result.code,new RegExp(`MaskType::${kind}`));
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-masks-'));
  try {
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),result.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);assert!(c.items().iter().any(|i|matches!(i,figleaf::paint::Item::Masked(_))));}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});
test('mask scope ends at a new mask, clipping frame or parent; hidden masks do not alter scope',()=>{
  const mask=node({id:'m',isMask:true,maskType:'ALPHA',opacity:.5});
  const r=exportHierarchy([node({type:'FRAME',fills:[],children:[mask,node({id:'a'}),node({id:'hidden',isMask:true,visible:false}),node({id:'b'}),{...mask,id:'m2'},node({id:'c'}),node({id:'clip',type:'FRAME',clipsContent:true}),node({id:'d'})]})]);
  assert.deepEqual(r.errors,[]);assert.equal((r.code.match(/\.masked\(/g)||[]).length,2);
  assert.equal((r.code.match(/masked_0.shape\(/g)||[]).length,2);assert.equal((r.code.match(/masked_1.shape\(/g)||[]).length,1);
  assert.match(r.code,/MaskType::Alpha, mask_0\).opacity\(0.5\)/);
  for(const patch of [{maskType:'OTHER'},{opacity:NaN},{opacity:2},{effects:[{type:'LAYER_BLUR'}]},{type:'TEXT',maskType:'VECTOR'}]) {
    const r=exportHierarchy([node({type:'FRAME',children:[{...mask,...patch},node()]})]);assert.equal(r.code,null);assert.ok(r.errors.length);
  }
});

test('mask bounds and unsupported vector sources fail before code is published',()=>{
  const mask=()=>node({isMask:true,maskType:'ALPHA'});
  const root=node({type:'FRAME',children:Array.from({length:33},mask)});
  assert.equal(exportHierarchy([root]).code,null);
  let nested=node();for(let i=0;i<9;i++)nested=node({type:'FRAME',children:[mask(),nested]});
  assert.equal(exportHierarchy([nested]).code,null);
  const vector=node({type:'GROUP',isMask:true,maskType:'VECTOR',children:[node({type:'TEXT'})]});
  assert.equal(exportHierarchy([node({type:'FRAME',children:[vector,node()]})]).code,null);
});

test('variable profiles and supported brushes export resolved outlines and compile',()=>{
  const r=exportHierarchy([require('./complex-strokes-reference.json')]);assert.deepEqual(r.errors,[]);
  for(const value of ['WEDGE','TAPER','CUSTOM','QUARTER_TAPER','EYE','MIRRORED_TAPER','HEIST','BLOCKBUSTER','Dynamic stroke'])assert.ok(r.code.includes(value));
  assert.equal((r.code.match(/\.stroke_geometry\(/g)||[]).length,9);
  assert.ok(!r.code.includes('.stroke_paths('));assert.ok(!r.code.includes('.stroke_cap('));
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'figleaf-complex-strokes-'));
  try{
    execFileSync('rustc',['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.resolve(__dirname,'../src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'main.rs'),r.code+'\nfn main(){let mut c=Canvas::new();draw_selection(&mut c);for i in c.items(){if let figleaf::paint::Item::Shape(s)=i{assert_eq!(s.problem(),None);}}}');
    execFileSync('rustc',['--edition=2024',path.join(tmp,'main.rs'),'--extern',`figleaf=${path.join(tmp,'libfigleaf.rlib')}`,'-o',path.join(tmp,'main')]);execFileSync(path.join(tmp,'main'));
  }finally{fs.rmSync(tmp,{recursive:true,force:true});}
});
test('styled strokes never fall back when outlines or style parameters are unsupported',()=>{
  const n=require('./complex-strokes-reference.json').children[0];
  for(const patch of [
    {strokeGeometry:undefined},{strokeGeometry:[]},{strokeGeometry:[{windingRule:'NONZERO',data:''}]},{fillGeometry:undefined},
    {strokeGeometry:[{windingRule:'NONE',data:'M0 0 L1 1'}]},
    {strokeGeometry:[{windingRule:'NONZERO',data:'M0 0 A1 1 0 0 0 1 1'}]},
    {strokeWeight:NaN},{strokeWeight:-1},{strokeAlign:'INSIDE'},
    {variableWidthStrokeProperties:{widthProfile:'UNKNOWN'}},
    ...[[],[{position:0,width:1}],[{position:0,width:1},{position:0,width:2}],[{position:0,width:-1},{position:1,width:1}],[{position:0,width:1},{position:2,width:1}]].map(variableWidthPoints=>({variableWidthStrokeProperties:{widthProfile:'CUSTOM',variableWidthPoints}})),
    {complexStrokeProperties:{type:'BRUSH',brushType:'SCATTER',brushName:'BUBBLEGUM'}},
    {complexStrokeProperties:{type:'BRUSH',brushType:'STRETCH',brushName:'CUSTOM',direction:'FORWARD'}},
    {complexStrokeProperties:{type:'BRUSH',brushType:'STRETCH',brushName:'HEIST',direction:'UNKNOWN'}},
    {complexStrokeProperties:{type:'DYNAMIC',frequency:1,wiggle:3,smoothen:.5}},
  ]) {const r=exportHierarchy([node({type:'FRAME',children:[node(),{...n,...patch}]})]);assert.equal(r.code,null,JSON.stringify(patch));assert.ok(r.errors.length);}
  const dynamic=require('./complex-strokes-reference.json').children[5];
  for(const complexStrokeProperties of [{type:'DYNAMIC',frequency:0,wiggle:0,smoothen:0},{type:'DYNAMIC',frequency:1,wiggle:NaN,smoothen:0},{type:'DYNAMIC',frequency:1,wiggle:0,smoothen:2}])assert.equal(exportHierarchy([{...dynamic,complexStrokeProperties}]).code,null);
  const line=exportHierarchy([{...n,type:'LINE'}]);assert.deepEqual(line.errors,[]);assert.match(line.code,/stroke_geometry/);
});
