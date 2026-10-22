/* Page-specific lazy module: backup. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['backup'] = {
  async mount() {
    if (typeof window.refreshRestorePoints === 'function') await window.refreshRestorePoints();
  }
};
