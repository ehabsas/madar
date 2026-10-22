/* Page-specific lazy module: dashboard. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['dashboard'] = {
  async mount() {
    if (typeof window.refreshDashboard === 'function') await window.refreshDashboard();
  }
};
