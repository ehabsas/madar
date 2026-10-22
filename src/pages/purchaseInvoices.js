/* Page-specific lazy module: purchaseInvoices. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['purchaseInvoices'] = {
  async mount() {
    if (typeof window.loadInvoices === 'function') await window.loadInvoices('purchase');
  }
};
