/* Page-specific lazy module: quotations. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['quotations'] = {
  async mount() {
    if (typeof window.loadQuotations === 'function') await window.loadQuotations();
  }
};
