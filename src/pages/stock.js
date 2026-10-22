/* Page-specific lazy module: stock. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['stock'] = {
  async mount() {
    if (typeof window.refreshStock === 'function') await window.refreshStock(undefined, true);
  }
};
