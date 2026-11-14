figma.showUI(__html__, { width: 720, height: 600, themeColors: true });
let revision = 0;
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
figma.ui.onmessage = message => {
  if (message.type === 'ready') invalidate();
  if (message.type === 'generate') {
    const result = exportHierarchy(figma.currentPage.selection, figma.root.documentColorProfile);
    figma.ui.postMessage({ type: 'result', revision, ...result });
  }
};
invalidate();
