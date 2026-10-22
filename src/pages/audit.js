/* Page-specific lazy module: audit. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['audit'] = {
  async mount() {
    if (typeof window.loadAuditLog === 'function') await window.loadAuditLog('');
  }
};
