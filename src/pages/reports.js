/* Page-specific lazy module: reports. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['reports'] = {
  async mount() {
    if (typeof window.refreshReports === 'function') await window.refreshReports();
  }
};
