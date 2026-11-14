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
  return { id: '1:1', name: 'Rectangle', type: 'RECTANGLE', visible: true, opacity: 1, blendMode: 'NORMAL', relativeTransform: [[1,0,0],[0,1,0]], width: 100, height: 40, fills: [solid], strokes: [], effects: [], cornerSmoothing: 0, topLeftRadius: 0, topRightRadius: 0, bottomLeftRadius: 0, bottomRightRadius: 0, strokeTopWeight: 0, strokeRightWeight: 0, strokeBottomWeight: 0, strokeLeftWeight: 0, strokeAlign: 'INSIDE', strokeJoin: 'MITER', ...props };
}
function text(props = {}) {
  return node({ type:'TEXT', name:'Label', characters:'Hello "Rust" \\ world', fontName:{family:'Inter',style:'Regular'}, fontSize:12, lineHeight:{unit:'PIXELS',value:16}, letterSpacing:{unit:'PIXELS',value:0}, textCase:'ORIGINAL', textDecoration:'NONE', textAlignHorizontal:'LEFT', textAlignVertical:'CENTER', ...props });
}
function fixture() {
  return node({ type:'FRAME', name:'Frame', relativeTransform:[[1,0,500],[0,1,200]], clipsContent:true, fills:[], strokes:[solid], strokeBottomWeight:1, children:[
    node({name:'Pill', relativeTransform:[[1,0,3],[0,1,4]], width:30, height:20, topLeftRadius:8,topRightRadius:8,bottomLeftRadius:8,bottomRightRadius:8, fills:[solid,{type:'GRADIENT_LINEAR', opacity:0.5, gradientTransform:[[1,0,0],[0,1,0]], gradientStops:[{position:0,color:{r:0,g:0,b:1,a:1}},{position:1,color:{r:0,g:1,b:0,a:0}}]}], effects:[{type:'DROP_SHADOW',color:{r:0,g:0,b:0,a:0.2},offset:{x:0,y:2},radius:4,spread:0}]}),
    text({relativeTransform:[[1,0,35],[0,1,4]],width:60,height:20}),
    node({name:'Dot',type:'ELLIPSE',width:8,height:8,relativeTransform:[[1,0,90],[0,1,30]]}),
    node({type:'LINE',name:'Line',width:10,height:0,fills:[],strokes:[solid],strokeWeight:2,strokeCap:'ROUND',relativeTransform:[[0,-1,15],[1,0,15]]}),
  ] });
}
test('deterministic complete hierarchy and Rust compilation', () => {
  const result = exportHierarchy([fixture()]);
  assert.deepEqual(result.errors, []);
  assert.equal(result.code, exportHierarchy([fixture()]).code);
  assert.match(result.code, /strokes\(\[0.0, 0.0, 1.0, 0.0\]/);
  assert.match(result.code, /Rect::new\(3.0, 4.0, 30.0, 20.0\)/);
  assert.match(result.code, /fonts\[0\]: Inter \/ Regular/);
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'figleaf-plugin-test-'));
  try {
    const root = path.resolve(__dirname, '..');
    execFileSync('rustc', ['--edition=2024','--crate-name=figleaf','--crate-type=rlib',path.join(root,'src/lib.rs'),'-o',path.join(tmp,'libfigleaf.rlib')]);
    fs.writeFileSync(path.join(tmp,'generated.rs'), result.code + '\nfn main() { let mut c = Canvas::new(); draw_selection(&mut c, [0]); assert_eq!(c.items().len(), 5); for item in c.items() { if let figleaf::paint::Item::Shape(s) = item { assert_eq!(s.problem(), None); } } }');
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
    node({relativeTransform:[[0,-1,0],[1,0,0]]}),
    node({strokes:[solid,solid]}), node({strokes:[solid],dashPattern:[1,2]}),
    node({strokes:[solid],strokeJoin:'BEVEL'}),
    node({fills:[{type:'GRADIENT_RADIAL'}]}),
    node({fills:[{type:'GRADIENT_LINEAR',gradientTransform:[[1,0,0],[0,1,0]],gradientStops:Array(9).fill({position:0,color:{r:0,g:0,b:0,a:1}})}]}),
    node({type:'ELLIPSE',arcData:{startingAngle:0,endingAngle:1,innerRadius:0}}),
    text({lineHeight:{unit:'AUTO'}}),text({textDecoration:'UNDERLINE'}),text({characters:'🐈'}),
    text({getStyledTextSegments:() => [{},{ }]}),text({paragraphSpacing:4}),text({textTruncation:'ENDING'}),
    text({getStyledTextSegments:() => [{openTypeFeatures:{liga:0}}]}),
    text({fills:[solid,solid]}),text({characters:'a'.repeat(513)}), text({fontSize:129}),
  ];
  for (const n of cases) { const r = exportHierarchy([n]); assert.equal(r.code,null,JSON.stringify(n)); assert.ok(r.errors.length); }
});
test('clips are intersected or rejected, never overwritten', () => {
  const r = exportHierarchy([node({type:'FRAME',clipsContent:true,width:50,height:50,children:[node({type:'FRAME',width:50,height:50,relativeTransform:[[1,0,20],[0,1,20]],clipsContent:true,children:[node()]})]})]);
  assert.deepEqual(r.errors,[]); assert.match(r.code,/clip_rounded\(Rect::new\(20.0, 20.0, 30.0, 30.0\), 0.0\)/);
  const round = node({type:'FRAME',clipsContent:true,topLeftRadius:8,topRightRadius:8,bottomLeftRadius:8,bottomRightRadius:8,children:[text()]});
  assert.equal(exportHierarchy([round]).code,null);
  round.children=[node({...round,children:[node()],relativeTransform:[[1,0,20],[0,1,20]]})];
  assert.equal(exportHierarchy([round]).code,null);
});
test('selection validation, hidden subtrees, and hostile names', () => {
  assert.equal(exportHierarchy([]).code,null); assert.equal(exportHierarchy([node(),node()]).code,null);
  const r = exportHierarchy([node({name:'x\n}\npanic!(); //', children:[node({visible:false,type:'VECTOR',children:[node({type:'IMAGE'})]})]})]);
  assert.deepEqual(r.errors,[]); assert.match(r.code,/\/\/ x } panic!\(\); \/\//);
});
test('plugin lifecycle is read-only, page-local, and invalidates results', () => {
  const events = {}, messages = [], pageEvents = {};
  const page = {selection:[fixture()],on:(key,fn)=>pageEvents[key]=fn,off:()=>{}};
  const figma = {root:{documentColorProfile:'SRGB'},currentPage:page,showUI:()=>{},on:(key,fn)=>events[key]=fn,ui:{postMessage:m=>messages.push(m)}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,'dist/code.js'),'utf8'),{figma});
  assert.equal(messages.at(-1).type,'invalidated');
  figma.ui.onmessage({type:'generate'}); assert.equal(messages.at(-1).errors.length,0);
  pageEvents.nodechange(); assert.equal(messages.at(-1).type,'invalidated');
  page.selection=[]; events.selectionchange(); figma.ui.onmessage({type:'generate'}); assert.equal(messages.at(-1).code,null);
  assert.equal(events.documentchange,undefined);
});
test('variable bindings are errors rather than fallback values', () => {
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
