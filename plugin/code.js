figma.showUI(__html__, { width: 720, height: 600, themeColors: true });
let revision = 0;
let generation = 0;
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
  if (message.type === 'ready') invalidate();
  if (message.type === 'generate') {
    const started = revision, request = ++generation;
    const selection = [...figma.currentPage.selection];
    const result = await exportWithVariables(selection, figma.root.documentColorProfile, figma.variables);
    if (started !== revision || request !== generation) return;
    figma.ui.postMessage({ type: 'result', revision: started, ...result });
  }
};
invalidate();
