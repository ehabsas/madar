/* Page-specific lazy module: parties. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['parties'] = {
  async mount() {
    if (typeof window.loadParties === 'function') await window.loadParties();
  }
};
