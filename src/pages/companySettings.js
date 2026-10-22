/* Page-specific lazy module: companySettings. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['companySettings'] = {
  async mount() {
    if (typeof window.loadCompanyProfile === 'function') await window.loadCompanyProfile();
  }
};
