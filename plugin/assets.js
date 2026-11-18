// Read image bytes and referenced pattern nodes. Decode in the UI's in-memory
// canvas; no exports, downloads, mutations or filesystem access.
async function exportWithAssets(selection,colorProfile,api,decode) {
  const resources={images:new Map(),patterns:new Map()},visiting=new Set();
  let count=0,pixels=0,context={path:'Selection assets'};
  async function visit(node,depth=0,path=node.name) {
    context={nodeId:node.id,path};
    if (++count>5000 || depth>128) throw Error('Asset hierarchy limit exceeded.');
    if(node.visible===false)return;
    const fills=node.type==='TEXT'&&node.getStyledTextSegments
      ?node.getStyledTextSegments(['fills']).flatMap(s=>Array.isArray(s.fills)?s.fills:[])
      :(Array.isArray(node.fills)?node.fills:[]);
    for(const p of [...fills,...(Array.isArray(node.strokes)?node.strokes:[])]) {
      if(p.visible===false)continue;
      context={nodeId:node.id,path};
      if(p.type==='IMAGE' && !resources.images.has(p.imageHash)) {
        const image=api.getImageByHash(p.imageHash);
        if(!image)throw Error(`Image ${p.imageHash} is unavailable.`);
        const size=await image.getSizeAsync();
        if(size.width*size.height>1000000 || (pixels+=size.width*size.height)>2000000)throw Error('Image export exceeds the pixel budget (1 million per image, 2 million total).');
        const bytes=await image.getBytesAsync();
        if(bytes.length>16000000)throw Error('Encoded image exceeds 16 MB.');
        const tag=(at,n)=>String.fromCharCode(...bytes.slice(at,at+n));
        const png=tag(1,3)==='PNG',jpeg=bytes[0]===255&&bytes[1]===216,webp=tag(0,4)==='RIFF'&&tag(8,4)==='WEBP';
        if(!png&&!jpeg&&!webp)throw Error('Only static PNG, JPEG and WebP images are supported.');
        if(png)for(let at=8;at+12<=bytes.length;){const size=bytes[at]*16777216+bytes[at+1]*65536+bytes[at+2]*256+bytes[at+3];if(tag(at+4,4)==='acTL')throw Error('Animated images are unsupported.');at+=size+12;}
        if(webp)for(let at=12;at+8<=bytes.length;){const size=bytes[at+4]+bytes[at+5]*256+bytes[at+6]*65536+bytes[at+7]*16777216;if(['ANIM','ANMF'].includes(tag(at,4)))throw Error('Animated images are unsupported.');at+=8+size+(size%2);}
        const result=await decode(bytes);
        if(result.width!==size.width || result.height!==size.height)throw Error('Decoded image dimensions differ from Figma.');
        resources.images.set(p.imageHash,result);
      }
      if(p.type==='PATTERN') {
        if(visiting.has(p.sourceNodeId) || visiting.size>=8)throw Error('Cyclic or deeply nested pattern source.');
        if(!resources.patterns.has(p.sourceNodeId)) {
          if(resources.patterns.size>=32)throw Error('More than 32 pattern sources.');
          const source=await api.getNodeByIdAsync(p.sourceNodeId);
          if(!source)throw Error('Pattern source is unavailable.');
          visiting.add(p.sourceNodeId);resources.patterns.set(p.sourceNodeId,source);await visit(source,depth+1,`${path} / Pattern source ${source.name}`);visiting.delete(p.sourceNodeId);
        }
      }
    }
    for(const child of node.children || [])await visit(child,depth+1,`${path} / ${child.name}`);
  }
  try {for(const root of selection)await visit(root);}
  catch(e){return {code:null,errors:[{...context,message:e.message}]};}
  return exportWithVariables(selection,colorProfile,api.variables,resources);
}
if(typeof module!=='undefined')module.exports={exportWithAssets};
