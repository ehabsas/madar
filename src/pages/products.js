/* Page-specific lazy module: products. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['products'] = {
  async mount() {
    if (typeof window.refreshProducts === 'function') await window.refreshProducts();
  }
};
