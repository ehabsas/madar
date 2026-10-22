/* Page-specific lazy module: salesInvoices. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['salesInvoices'] = {
  async mount() {
    if (typeof window.loadInvoices === 'function') await window.loadInvoices('sale');
  }
};
