/* Madar shell + on-demand page templates. Each page fragment is fetched once and
   its DOM node is cached so switching pages preserves form state and listeners. */
(() => {
  const pageNames = [
    "dashboard", "products", "stock", "parties", "purchaseInvoices",
    "salesInvoices", "quotations", "finance", "reports", "companySettings",
    "backup", "users", "audit", "help"
  ];
  const knownPages = new Set(pageNames);
  const pageNodes = new Map();
  const pagePromises = new Map();
  const modulePromises = new Map();
  const stylesheetLinks = new Map();
  const host = document.getElementById("pageHost");
  let activePage = null;

  async function fetchPage(name) {
    if (!knownPages.has(name)) throw new Error(`صفحة غير معروفة: ${name}`);
    if (pageNodes.has(name)) return pageNodes.get(name);
    if (pagePromises.has(name)) return pagePromises.get(name);
    const promise = (async () => {
      const response = await fetch(`./pages/${name}.html`);
      if (!response.ok) throw new Error(`تعذر تحميل صفحة ${name} (${response.status})`);
      const html = await response.text();
      const template = document.createElement("template");
      template.innerHTML = html.trim();
      const page = template.content.querySelector("[data-page-content]");
      if (!page || page.dataset.pageContent !== name) {
        throw new Error(`قالب الصفحة ${name} غير صالح أو لا يطابق اسمها`);
      }
      pageNodes.set(name, page);
      return page;
    })();
    pagePromises.set(name, promise);
    try { return await promise; }
    catch (error) { pagePromises.delete(name); throw error; }
  }

  async function loadPageAssets(name) {
    if (!stylesheetLinks.has(name)) {
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = `./pages/${name}.css`;
      link.dataset.madarPageStyle = name;
      link.onerror = () => console.warn(`[Madar] Optional page stylesheet unavailable: ${name}`);
      document.head.appendChild(link);
      stylesheetLinks.set(name, link);
    }
    if (!modulePromises.has(name)) {
      const promise = new Promise((resolve, reject) => {
        const script = document.createElement("script");
        script.src = `./pages/${name}.js`;
        script.dataset.madarPageScript = name;
        script.onload = () => resolve();
        script.onerror = () => reject(new Error(`تعذر تحميل منطق الصفحة ${name}`));
        document.body.appendChild(script);
      });
      modulePromises.set(name, promise);
      promise.catch(() => modulePromises.delete(name));
    }
    await modulePromises.get(name);
  }

  window.loadMadarPage = async function loadMadarPage(name) {
    const page = await fetchPage(name);
    await loadPageAssets(name);
    // Templates may carry `hidden` in their source to avoid flashing before
    // authorization. Once selected, reveal the root; prepareMadarPage then
    // reapplies permission visibility for protected page roots.
    page.hidden = false;
    if (activePage !== name || host.firstElementChild !== page) {
      host.replaceChildren(page);
      activePage = name;
      document.dispatchEvent(new CustomEvent("madar:page-loaded", { detail: { page: name } }));
    }
    if (typeof window.bindMadarPageHandlers === "function") window.bindMadarPageHandlers(page);
    if (typeof window.prepareMadarPage === "function") window.prepareMadarPage(name, page);
    const module = window.MadarPageModules && window.MadarPageModules[name];
    if (module && typeof module.mount === "function") await module.mount();
    return page;
  };

  // Only the dashboard template is requested during startup.
  fetchPage("dashboard").then(page => {
    host.replaceChildren(page);
    activePage = "dashboard";
    const script = document.createElement("script");
    script.src = "./app.js";
    script.onerror = () => {
      host.innerHTML = '<section class="panel"><h2>تعذر تحميل مدار</h2><p class="status">تعذر تحميل ملف تشغيل التطبيق (app.js).</p></section>';
    };
    document.body.appendChild(script);
  }).catch(error => {
    console.error("Madar startup failed:", error);
    host.innerHTML = `<section class="panel"><h2>تعذر تحميل مدار</h2><p class="muted">${String(error.message || error)}</p><p class="muted">تأكد من تشغيل التطبيق ثم أعد المحاولة.</p></section>`;
  });
})();
