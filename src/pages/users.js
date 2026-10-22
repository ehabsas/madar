/* Page-specific lazy module: users. */
window.MadarPageModules = window.MadarPageModules || Object.create(null);
window.MadarPageModules['users'] = {
  async mount() {
    if (typeof window.loadUsers === 'function') await window.loadUsers();
  }
};
