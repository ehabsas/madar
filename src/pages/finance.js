/* Page-specific lazy module: finance. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['finance'] = {
  async mount() {
    if (typeof window.loadFinance === 'function') await window.loadFinance();
  }
};
