figma.showUI(__html__, { width: 720, height: 600, themeColors: true });
let revision = 0;
let generation = 0;
let decodeId=0;
const decodes=new Map();
function decodeImage(bytes) {
  return new Promise((resolve,reject)=>{const id=++decodeId;decodes.set(id,{resolve,reject});figma.ui.postMessage({type:'decode-image',id,bytes});});
}
function invalidate() {
  revision++;
  figma.ui.postMessage({ type: 'invalidated', revision, selection: figma.currentPage.selection.map(n => n.name) });
}
figma.on('selectionchange', invalidate);
// Page-local events work with dynamic-page access without loading the whole file.
let page = figma.currentPage;
page.on('nodechange', invalidate);
figma.on('stylechange', invalidate);
figma.on('currentpagechange', () => {
  page.off('nodechange', invalidate);
  page = figma.currentPage;
  page.on('nodechange', invalidate);
  invalidate();
});
figma.ui.onmessage = async message => {
  if(message.type==='decoded-image') {const pending=decodes.get(message.id);if(pending){decodes.delete(message.id);if(message.error)pending.reject(Error(message.error));else pending.resolve(message.image);}return;}
  if (message.type === 'ready') invalidate();
  if (message.type === 'generate') {
    const started = revision, request = ++generation;
    const selection = [...figma.currentPage.selection];
    const result = await exportWithAssets(selection, figma.root.documentColorProfile, figma, decodeImage);
    if (started !== revision || request !== generation) return;
    figma.ui.postMessage({ type: 'result', revision: started, ...result });
  }
};
invalidate();
