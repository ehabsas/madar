
// Page-aware event registration: handlers for lazy-loaded templates are queued until their elements exist.
const madarPendingElementHandlers = new Map();
const madarBoundElementHandlers = new WeakMap();
function onElement(id, eventName, handler) {
  const entry = { eventName, handler };
  if (!madarPendingElementHandlers.has(id)) madarPendingElementHandlers.set(id, []);
  madarPendingElementHandlers.get(id).push(entry);
  const element = document.getElementById(id);
  if (element) bindMadarElementHandlers(element, id);
}
function bindMadarElementHandlers(element, id = element.id) {
  if (!element || !id) return;
  const entries = madarPendingElementHandlers.get(id) || [];
  let bound = madarBoundElementHandlers.get(element);
  if (!bound) { bound = new Set(); madarBoundElementHandlers.set(element, bound); }
  for (const entry of entries) {
    if (bound.has(entry)) continue;
    element.addEventListener(entry.eventName, entry.handler);
    bound.add(entry);
  }
}
function bindMadarPageHandlers(root) {
  if (!root) return;
  if (root.id) bindMadarElementHandlers(root, root.id);
  root.querySelectorAll('[id]').forEach(element => bindMadarElementHandlers(element, element.id));
}
window.bindMadarPageHandlers = bindMadarPageHandlers;
window.prepareMadarPage = function prepareMadarPage(page, root) {
  statusEl = document.getElementById('status');
  productViewToggle = document.getElementById('productViewToggle');
  productViewHelp = document.getElementById('productViewHelp');
  backupStatus = document.getElementById('backupStatus');
  restorePointsEl = document.getElementById('restorePoints');
  if (typeof applyElementPermissions === 'function') applyElementPermissions();
  if (page === 'products') updateProductViewControls();
  if (currentLanguage === 'en' && typeof translateDom === 'function') translateDom(root);
};

// Avoid WebKitGTK's native clipboard paths on Linux: this app owns copy/cut/paste
// for text controls and suppresses the page context menu (including Inspect Element).
const madarEditMenu=document.getElementById('madarEditContextMenu');
let madarEditTarget=null;
function getTextEditTarget(node){
 const el=node instanceof Element?node.closest('input,textarea,[contenteditable="true"],[contenteditable=""]'):null;
 if(!el||el.disabled)return null;
 if(el instanceof HTMLInputElement && ['button','submit','reset','checkbox','radio','file','range','color','image','hidden'].includes((el.type||'text').toLowerCase()))return null;
 return el;
}
function isEditableTarget(el){return !!el && !el.disabled && !el.readOnly && el.getAttribute('aria-disabled')!=='true';}
function selectedTextForTarget(el){
 if(!el)return '';
 if(el instanceof HTMLInputElement||el instanceof HTMLTextAreaElement){
  const start=el.selectionStart,end=el.selectionEnd;
  return typeof start==='number'&&typeof end==='number'&&end>start?el.value.slice(start,end):'';
 }
 if(el.isContentEditable){const sel=window.getSelection();return sel?sel.toString():'';}
 return '';
}
function refreshMadarEditMenuLabels(){
 if(!madarEditMenu)return;
 const en=currentLanguage==='en';
 const labels=en?{cut:'Cut',copy:'Copy',paste:'Paste',selectAll:'Select all'}:{cut:'قص',copy:'نسخ',paste:'لصق',selectAll:'تحديد الكل'};
 madarEditMenu.querySelectorAll('[data-edit-label]').forEach(node=>{node.textContent=labels[node.dataset.editLabel]||node.textContent;});
 madarEditMenu.setAttribute('aria-label',en?'Text editing':'تحرير النص');
 madarEditMenu.style.direction=en?'ltr':'rtl';
 madarEditMenu.querySelectorAll('.edit-menu-shortcut').forEach(node=>{node.textContent=(navigator.platform||'').toLowerCase().includes('mac')?node.textContent.replace('Ctrl','⌘'):node.textContent.replace('⌘','Ctrl');});
}
function hideMadarEditMenu(){if(madarEditMenu)madarEditMenu.hidden=true;madarEditTarget=null;}
function showMadarEditMenu(event,target){
 madarEditTarget=target;refreshMadarEditMenuLabels();
 const canEdit=isEditableTarget(target),hasSelection=selectedTextForTarget(target).length>0;
 madarEditMenu.querySelector('[data-edit-action="cut"]').disabled=!canEdit||!hasSelection;
 madarEditMenu.querySelector('[data-edit-action="copy"]').disabled=!hasSelection;
 madarEditMenu.querySelector('[data-edit-action="paste"]').disabled=!canEdit;
 const alreadyAll=target instanceof HTMLInputElement||target instanceof HTMLTextAreaElement
  ?(typeof target.selectionStart==='number'&&target.selectionStart===0&&target.selectionEnd===target.value.length)
  :false;
 madarEditMenu.querySelector('[data-edit-action="selectAll"]').disabled=alreadyAll;
 madarEditMenu.hidden=false;
 const rect=madarEditMenu.getBoundingClientRect();
 const x=Math.max(4,Math.min(event.clientX,window.innerWidth-rect.width-4));
 const y=Math.max(4,Math.min(event.clientY,window.innerHeight-rect.height-4));
 madarEditMenu.style.left=x+'px';madarEditMenu.style.top=y+'px';
}
function normalizePastedText(target,text){
 if(['barcode','editBarcode','alternateBarcodeValue','invoiceBarcodeScan'].includes(target.id))return text.replace(/[\u0000-\u001F\u007F]/g,'').trim();
 if(target instanceof HTMLInputElement)return text.replace(/[\r\n]+/g,' ');
 return text;
}
function dispatchEditedInput(target,inputType,data){
 try{target.dispatchEvent(new InputEvent('input',{bubbles:true,inputType,data}));}
 catch(_){target.dispatchEvent(new Event('input',{bubbles:true}));}
}
async function readMadarClipboard(){
 if(!invoke)return '';
 try{const text=await invoke('clipboard_read_text');return typeof text==='string'?text:'';}
 catch(error){console.warn('[Madar] Clipboard text unavailable:',error);return '';}
}
async function writeMadarClipboard(text){
 if(!invoke)return false;
 try{await invoke('clipboard_write_text',{text:String(text)});return true;}
 catch(error){console.warn('[Madar] Clipboard write failed:',error);return false;}
}
async function pasteIntoMadarTarget(target){
 if(!isEditableTarget(target))return;
 const raw=await readMadarClipboard();if(!raw)return;
 const text=normalizePastedText(target,raw);
 if(target instanceof HTMLInputElement||target instanceof HTMLTextAreaElement){
  const start=typeof target.selectionStart==='number'?target.selectionStart:target.value.length;
  const end=typeof target.selectionEnd==='number'?target.selectionEnd:target.value.length;
  const max=Number.isFinite(target.maxLength)&&target.maxLength>=0?target.maxLength:Infinity;
  const remaining=max-(target.value.length-(end-start));
  const inserted=text.slice(0,Math.max(0,remaining));
  target.value=target.value.slice(0,start)+inserted+target.value.slice(end);
  const caret=start+inserted.length;
  try{target.setSelectionRange(caret,caret);}catch(_){}
  dispatchEditedInput(target,'insertFromPaste',inserted);
  return;
 }
 if(target.isContentEditable){
  target.focus();const sel=window.getSelection();
  if(sel&&sel.rangeCount){const range=sel.getRangeAt(0);if(target.contains(range.commonAncestorContainer)||range.commonAncestorContainer===target){range.deleteContents();const node=document.createTextNode(text);range.insertNode(node);range.setStartAfter(node);range.collapse(true);sel.removeAllRanges();sel.addRange(range);dispatchEditedInput(target,'insertFromPaste',text);}}
 }
}
async function copyOrCutMadarTarget(target,cut){
 const selected=selectedTextForTarget(target);if(!selected)return;
 if(!await writeMadarClipboard(selected))return;
 if(!cut||!isEditableTarget(target))return;
 if(target instanceof HTMLInputElement||target instanceof HTMLTextAreaElement){
  const start=target.selectionStart,end=target.selectionEnd;
  if(typeof start!=='number'||typeof end!=='number')return;
  target.value=target.value.slice(0,start)+target.value.slice(end);try{target.setSelectionRange(start,start);}catch(_){}dispatchEditedInput(target,'deleteByCut',null);
 }else if(target.isContentEditable){const sel=window.getSelection();if(sel&&sel.rangeCount){sel.deleteFromDocument();dispatchEditedInput(target,'deleteByCut',null);}}
}
window.addEventListener('contextmenu',function(event){
 const target=getTextEditTarget(event.target);
 event.preventDefault();event.stopPropagation();
 if(target)showMadarEditMenu(event,target);else hideMadarEditMenu();
},{capture:true});
// Prevent WebKit's native Ctrl/Command+V/C/X handlers before they query GTK clipboard formats.
document.addEventListener('keydown',function(event){
 const target=getTextEditTarget(event.target);
 if(event.key==='Escape'&&!madarEditMenu.hidden){hideMadarEditMenu();return;}
 if(!target)return;
 const key=event.key.toLowerCase(),mod=event.ctrlKey||event.metaKey;
 let action=null;
 if(mod&&key==='v')action='paste';else if(mod&&key==='c')action='copy';else if(mod&&key==='x')action='cut';else if(event.shiftKey&&event.key==='Insert')action='paste';else if(event.ctrlKey&&event.key==='Insert')action='copy';
 if(!action)return;
 event.preventDefault();event.stopImmediatePropagation();hideMadarEditMenu();
 if(action==='paste')void pasteIntoMadarTarget(target);else void copyOrCutMadarTarget(target,action==='cut');
},{capture:true});
// All field-menu actions use the Rust clipboard bridge instead of browser-native clipboard access.
madarEditMenu.addEventListener('mousedown',event=>event.preventDefault());
madarEditMenu.addEventListener('click',event=>{
 const button=event.target.closest('[data-edit-action]');if(!button||!madarEditTarget)return;
 const target=madarEditTarget,action=button.dataset.editAction;hideMadarEditMenu();target.focus();
 if(action==='paste')void pasteIntoMadarTarget(target);
 else if(action==='copy')void copyOrCutMadarTarget(target,false);
 else if(action==='cut')void copyOrCutMadarTarget(target,true);
 else if(action==='selectAll'){
  if(target instanceof HTMLInputElement||target instanceof HTMLTextAreaElement){try{target.select();}catch(_){}}
  else if(target.isContentEditable){const range=document.createRange();range.selectNodeContents(target);const sel=window.getSelection();sel?.removeAllRanges();sel?.addRange(range);}
 }
});
document.addEventListener('pointerdown',event=>{if(!madarEditMenu.hidden&&!madarEditMenu.contains(event.target))hideMadarEditMenu();},true);
document.addEventListener('focusin',event=>{if(!madarEditMenu.hidden&&!madarEditMenu.contains(event.target)&&event.target!==madarEditTarget)hideMadarEditMenu();},true);
window.addEventListener('blur',hideMadarEditMenu);window.addEventListener('resize',hideMadarEditMenu);document.addEventListener('scroll',hideMadarEditMenu,true);
// Navigation: show one section at a time without reloading the app.
async function showPage(page){const required={'products':'products.view','stock':'stock.view','users':'users.view','audit':'audit.view','purchaseInvoices':'purchases.view','salesInvoices':'sales.view','quotations':'quotations.view','finance':'finance.view','reports':'reports.view'}[page];const backupPermissions=['backup.view_points','backup.create','backup.restore','backup.points.delete'];const allowed=page==='companySettings'?(!!currentUser&&['company.settings.view','company.settings.manage'].some(p=>currentUser.permissions.includes(p))):page==='backup'?(!!currentUser&&backupPermissions.some(p=>currentUser.permissions.includes(p))):page==='parties'?(!!currentUser&&['customers.view','suppliers.view'].some(p=>currentUser.permissions.includes(p))):(!required||(!!currentUser&&currentUser.permissions.includes(required)));if(!allowed){if(statusEl)statusEl.textContent=currentLanguage==='en'?'You do not have permission to open this section.':'ليس لديك صلاحية لفتح هذا القسم.';return;}try{await window.loadMadarPage(page);}catch(error){console.error('[Madar] page load failed:',error);if(statusEl)statusEl.textContent=(currentLanguage==='en'?'Could not load page: ':'تعذر تحميل الصفحة: ')+String(error.message||error);return;}document.querySelectorAll('.nav-btn').forEach(el=>{const active=el.dataset.page===page;el.classList.toggle('active',active);if(active)el.setAttribute('aria-current','page');else el.removeAttribute('aria-current');});if(page==='products')setTimeout(()=>document.getElementById('productSearch')?.focus(),0);}

document.querySelectorAll('.nav-btn').forEach(btn=>btn.addEventListener('click',()=>showPage(btn.dataset.page)));
document.addEventListener('click',event=>{const btn=event.target.closest('[data-go-page]');if(btn)showPage(btn.dataset.goPage);});
// Theme and language preferences are saved locally on this device.
let currentLanguage='ar';try{currentLanguage=localStorage.getItem('madar-language')==='en'?'en':'ar';}catch(_){}
const themeToggle=document.getElementById('themeToggle');
function applyTheme(theme){document.documentElement.dataset.theme=theme;const dark=theme==='dark';themeToggle.textContent=currentLanguage==='en'?(dark?'☀️ Light mode':'🌙 Dark mode'):(dark?'☀️ الوضع الفاتح':'🌙 الوضع الداكن');themeToggle.setAttribute('aria-pressed',String(dark));}
let savedTheme='light';try{savedTheme=localStorage.getItem('madar-theme')||'light';}catch(_){}
applyTheme(savedTheme==='dark'?'dark':'light');

themeToggle.addEventListener('click',()=>{const next=document.documentElement.dataset.theme==='dark'?'light':'dark';applyTheme(next);try{localStorage.setItem('madar-theme',next);}catch(_){};closeHeaderMenus();});
const preferencesToggle=document.getElementById('preferencesToggle'),preferencesMenu=document.getElementById('preferencesMenu');
const userMenuToggle=document.getElementById('currentUserBadge'),userMenu=document.getElementById('userMenu');
function setDropdown(shell,menu,trigger,open){menu.hidden=!open;trigger.setAttribute('aria-expanded',String(open));shell.classList.toggle('dropdown-open',open);}
function closeHeaderMenus(){if(preferencesMenu)setDropdown(document.getElementById('preferencesShell'),preferencesMenu,preferencesToggle,false);if(userMenu)setDropdown(document.getElementById('userMenuShell'),userMenu,userMenuToggle,false);}
preferencesToggle.addEventListener('click',e=>{e.stopPropagation();const open=preferencesMenu.hidden;closeHeaderMenus();setDropdown(document.getElementById('preferencesShell'),preferencesMenu,preferencesToggle,open);});
userMenuToggle.addEventListener('click',e=>{e.stopPropagation();if(userMenuToggle.hidden)return;const open=userMenu.hidden;closeHeaderMenus();setDropdown(document.getElementById('userMenuShell'),userMenu,userMenuToggle,open);});
document.addEventListener('click',e=>{if(!e.target.closest('#preferencesShell')&&!e.target.closest('#userMenuShell'))closeHeaderMenus();});
document.addEventListener('keydown',e=>{if(e.key==='Escape')closeHeaderMenus();});
function renderHeaderUser(user){
 const trigger=document.getElementById('currentUserBadge');if(!user){trigger.hidden=true;document.getElementById('userMenuName').textContent='';document.getElementById('userMenuUsername').textContent='';document.getElementById('userMenuRole').textContent='';return;}
 const role=localizedRoleLabel(user.roleCode,user.roleName);const name=user.displayName||user.username||'';
 document.getElementById('headerUserName').textContent=name;document.getElementById('headerUserRole').textContent=role;
 document.getElementById('userMenuName').textContent=name;document.getElementById('userMenuUsername').textContent='@'+(user.username||'');document.getElementById('userMenuRole').textContent=role;
 const avatar=document.getElementById('headerUserAvatar');const url=typeof user.avatarDataUrl==='string'&&/^data:image\/(png|jpeg|webp);base64,/.test(user.avatarDataUrl)?user.avatarDataUrl:null;
 avatar.innerHTML=url?`<img alt="" src="${escapeHtml(url)}"/>`:'<svg viewBox="0 0 64 64" aria-hidden="true"><circle cx="32" cy="22" r="12" fill="currentColor"/><path d="M8 58c1-13 10-21 24-21s23 8 24 21" fill="currentColor"/></svg>';
 trigger.hidden=false;document.getElementById('dashboardUserRole').textContent=role;
}
function renderHeaderCompany(company){
 const box=document.getElementById('headerCompanyBrand');if(!company||!company.name){box.hidden=true;return;}
 document.getElementById('headerCompanyName').textContent=company.name;
 const logo=document.getElementById('headerCompanyLogo'),fallback=document.getElementById('headerCompanyLogoFallback');
 const url=typeof company.logoDataUrl==='string'&&/^data:image\/(png|jpeg|webp);base64,/.test(company.logoDataUrl)?company.logoDataUrl:'';
 if(url){logo.src=url;logo.hidden=false;fallback.hidden=true;}else{logo.removeAttribute('src');logo.hidden=true;fallback.hidden=false;}
 box.hidden=false;
}
async function loadHeaderBranding(){try{const c=await invoke('get_company_branding');currentCurrency=c.currencyCode||'EGP';renderHeaderCompany(c);}catch(_){renderHeaderCompany(null);}}


const invoke = window.MadarAPI.invoke;
let currentUser = null;
if(!window.MadarAPI.isDesktop){const note=document.createElement('div');note.className='browser-mode-banner';note.textContent='وضع المتصفح التجريبي — البيانات محلية لهذا المتصفح فقط؛ الفواتير والمخزون والنسخ الاحتياطي تتطلب تطبيق سطح المكتب.';document.body.prepend(note);}
let statusEl=document.getElementById('status');
const money=n=>(Number(n||0)/100).toFixed(2);
let viewingInactiveProducts=false;
let productViewToggle=document.getElementById('productViewToggle');
let productViewHelp=document.getElementById('productViewHelp');
function updateProductViewControls(){
 if(!productViewToggle||!productViewHelp)return;
 productViewToggle.textContent=viewingInactiveProducts?'العودة إلى الأصناف النشطة':'عرض الأصناف الموقوفة';
 productViewHelp.textContent=viewingInactiveProducts
  ?'أنت تعرض الأصناف الموقوفة. هذه الأصناف لم تُحذف؛ بياناتها وباركوداتها محفوظة. استخدم «إعادة تفعيل» لإرجاع الصنف إلى قائمة الأصناف النشطة.'
  :'أنت تعرض الأصناف النشطة. الأصناف التي توقفها لا تُحذف، ويمكنك العثور عليها من زر «عرض الأصناف الموقوفة» وإعادة تفعيلها في أي وقت.';
}
function confirmProductAction(message,title='تأكيد الإجراء',confirmLabel='نعم، نفّذ'){
 return new Promise(resolve=>{
  const overlay=document.getElementById('productActionOverlay');
  const ok=document.getElementById('productActionOk');
  const cancel=document.getElementById('productActionCancel');
  document.getElementById('productActionTitle').textContent=title;
  document.getElementById('productActionMessage').textContent=message;
  ok.textContent=confirmLabel;
  overlay.hidden=false;
  const finish=value=>{overlay.hidden=true;ok.removeEventListener('click',onOk);cancel.removeEventListener('click',onCancel);resolve(value);};
  const onOk=()=>finish(true);
  const onCancel=()=>finish(false);
  ok.addEventListener('click',onOk,{once:true});
  cancel.addEventListener('click',onCancel,{once:true});
  cancel.focus();
 });
}
onElement('productViewToggle','click',async()=>{
 viewingInactiveProducts=!viewingInactiveProducts;
 updateProductViewControls();
 await refreshProducts();
});
async function refreshProducts(query){
 if(!invoke){statusEl.textContent='واجهة المعاينة فقط: افتح التطبيق داخل Tauri لتفعيل حفظ البيانات.';return;}
 try{
  const term=query??document.getElementById('productSearch')?.value??'';
  const command=term.trim()
   ?(viewingInactiveProducts?'search_inactive_products':'search_products')
   :(viewingInactiveProducts?'list_inactive_products':'list_products');
  const args=term.trim()?{query:term}:undefined;
  const rows=args?await invoke(command,args):await invoke(command);
  const emptyMessage=viewingInactiveProducts?'لا توجد أصناف موقوفة مطابقة للبحث.':'لا توجد أصناف نشطة مطابقة للبحث.';
  document.getElementById('products').innerHTML=rows.map(p=>{
   const actions=viewingInactiveProducts
    ?`<button class="btn" data-permission="products.reactivate" data-reactivate-id="${p.id}" data-name="${escapeHtml(p.name)}" type="button">إعادة تفعيل</button>`
    :`<button class="btn secondary product-edit" data-permission="products.edit" data-id="${p.id}" type="button">تعديل</button><button class="btn secondary product-alt" data-permission="products.barcode.manage" data-id="${p.id}" type="button">باركود بديل</button><button class="btn danger product-deactivate" data-permission="products.deactivate" data-id="${p.id}" data-name="${escapeHtml(p.name)}" type="button">إيقاف</button>`;
   return `<tr><td>${escapeHtml(p.code)}</td><td>${escapeHtml(p.name)}</td><td>${escapeHtml(p.unit)}</td><td><code>${escapeHtml(p.barcode)}</code></td><td>${money(p.purchase_price_minor)}</td><td>${money(p.sale_price_minor)}</td><td><div class="actions" style="margin:0">${actions}</div></td></tr>`;
  }).join('')||`<tr><td colspan="7" class="muted">${emptyMessage}</td></tr>`;
  applyElementPermissions();
  const count=document.getElementById('dashboardProductCount');
  if(count&&!term.trim()&&!viewingInactiveProducts)count.textContent=rows.length.toLocaleString(currentLanguage==='en'?'en-US':'ar-EG');
  document.querySelectorAll('.product-edit').forEach(btn=>btn.addEventListener('click',()=>openProductEditor(rows.find(p=>p.id===Number(btn.dataset.id)))));
  document.querySelectorAll('.product-alt').forEach(btn=>btn.addEventListener('click',()=>{
   const p=rows.find(x=>x.id===Number(btn.dataset.id));
   if(!p)return;
   const form=document.getElementById('alternateBarcodeForm');
   form.reset();
   document.getElementById('alternateProductId').value=p.id;
   document.getElementById('alternateProductUnit').value=p.unit;
   document.getElementById('alternateBarcodeTitle').textContent=`باركود بديل — ${p.name}`;
   document.getElementById('alternateBarcodeHelp').textContent=`الصنف: ${p.name} | الكود: ${p.code}. أدخل باركودًا إضافيًا؛ لن يتغير الباركود الأساسي.`;
   document.getElementById('alternateBarcodeStatus').textContent='';
   document.getElementById('alternateBarcodeOverlay').hidden=false;
   document.getElementById('alternateBarcodeValue').focus();
  }));
  document.querySelectorAll('.product-deactivate').forEach(btn=>btn.addEventListener('click',async()=>{
   const id=Number(btn.dataset.id);const name=btn.dataset.name;
   const confirmed=await confirmProductAction(
    `هل تريد إيقاف الصنف «${name}»؟\n\nلن يتم حذف الصنف أو باركوداته. بعد التأكيد سينتقل إلى قائمة «الأصناف الموقوفة»، ويمكنك إعادة تفعيله لاحقًا.`,
    'تأكيد إيقاف الصنف','نعم، أوقف الصنف');
   if(!confirmed){statusEl.textContent='تم إلغاء العملية؛ لم يتم تغيير حالة الصنف.';return;}
   btn.disabled=true;
   try{
    await invoke('deactivate_product',{id});
    viewingInactiveProducts=true;updateProductViewControls();
    statusEl.textContent=`تم إيقاف «${name}». الصنف محفوظ الآن في قائمة الأصناف الموقوفة، ويمكنك إعادة تفعيله من هناك.`;
    await refreshProducts();
   }catch(err){statusEl.textContent='تعذر إيقاف الصنف: '+err;btn.disabled=false;}
  }));
  document.querySelectorAll('[data-reactivate-id]').forEach(btn=>btn.addEventListener('click',async()=>{
   const id=Number(btn.dataset.reactivateId);const name=btn.dataset.name;
   const confirmed=await confirmProductAction(
    `هل تريد إعادة تفعيل الصنف «${name}»؟\n\nسيعود إلى قائمة الأصناف النشطة وستظل بياناته وباركوداته كما هي.`,
    'تأكيد إعادة تفعيل الصنف','نعم، أعد التفعيل');
   if(!confirmed){statusEl.textContent='تم إلغاء العملية؛ ظل الصنف موقوفًا.';return;}
   btn.disabled=true;
   try{
    await invoke('reactivate_product',{id});
    viewingInactiveProducts=false;updateProductViewControls();
    statusEl.textContent=`تمت إعادة تفعيل «${name}» وإعادته إلى قائمة الأصناف النشطة.`;
    await refreshProducts();
   }catch(err){statusEl.textContent='تعذرت إعادة تفعيل الصنف: '+err;btn.disabled=false;}
  }));
 }catch(e){statusEl.textContent='تعذر تحميل الأصناف: '+e;}
}
updateProductViewControls();
function escapeHtml(v){return String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function openProductEditor(p){if(!p)return;document.getElementById('editId').value=p.id;document.getElementById('editCode').value=p.code;document.getElementById('editName').value=p.name;document.getElementById('editUnit').value=p.unit;document.getElementById('editBarcode').value=String(p.barcode??'');document.getElementById('editPurchase').value=money(p.purchase_price_minor);document.getElementById('editSale').value=money(p.sale_price_minor);document.getElementById('editProductStatus').textContent='';document.getElementById('editProductStatus').className='status';applyPriceFieldPermissions();document.getElementById('editProductOverlay').hidden=false;document.getElementById('editCode').focus();}
onElement('openAddProduct','click',()=>{
 const form=document.getElementById('productForm');form.reset();
 document.getElementById('unit').value='قطعة';
 document.getElementById('purchase').value='0';document.getElementById('sale').value='0';
 document.getElementById('addProductModalStatus').textContent='';
 document.getElementById('addProductOverlay').hidden=false;
 document.getElementById('code').focus();
});
onElement('cancelAddProduct','click',()=>{document.getElementById('addProductOverlay').hidden=true;});
onElement('cancelAlternateBarcode','click',()=>{document.getElementById('alternateBarcodeOverlay').hidden=true;});
onElement('productForm','submit',async e=>{
 e.preventDefault();
 const button=document.getElementById('saveNewProduct');const modalStatus=document.getElementById('addProductModalStatus');
 if(!invoke){modalStatus.textContent='الحفظ متاح عند تشغيل التطبيق داخل Tauri.';return;}
 button.disabled=true;modalStatus.textContent='جارٍ حفظ الصنف...';
 try{
  const p=await invoke('create_product',{code:document.getElementById('code').value,name:document.getElementById('name').value,unit:document.getElementById('unit').value,barcode:document.getElementById('barcode').value||null,purchasePriceMinor:Math.round(Number(document.getElementById('purchase').value)*100),salePriceMinor:Math.round(Number(document.getElementById('sale').value)*100)});
  statusEl.textContent=`تم حفظ الصنف: ${p.name}
الباركود: ${p.barcode}`;
  document.getElementById('productSearch').value='';
  viewingInactiveProducts=false;updateProductViewControls();
  document.getElementById('addProductOverlay').hidden=true;document.getElementById('addProductModalStatus').textContent='';
  document.getElementById('productForm').reset();document.getElementById('unit').value='قطعة';document.getElementById('purchase').value='0';document.getElementById('sale').value='0';
  await refreshProducts('');
 }catch(err){modalStatus.textContent='فشل حفظ الصنف: '+err;}
 finally{button.disabled=false;}
});
onElement('alternateBarcodeForm','submit',async e=>{
 e.preventDefault();
 const button=document.getElementById('saveAlternateBarcode');const status=document.getElementById('alternateBarcodeStatus');
 if(!invoke){status.textContent='إضافة الباركود متاحة عند تشغيل التطبيق داخل Tauri.';return;}
 const barcode=document.getElementById('alternateBarcodeValue').value.trim();
 if(!barcode){status.textContent='أدخل الباركود البديل أولًا.';return;}
 button.disabled=true;status.textContent='جارٍ حفظ الباركود البديل...';
 try{
  const result=await invoke('add_alternate_barcode',{productId:Number(document.getElementById('alternateProductId').value),barcode,unit:document.getElementById('alternateProductUnit').value});
  status.textContent=result+' يمكنك إدخال باركود بديل آخر للصنف نفسه.';
  document.getElementById('alternateBarcodeValue').value='';
  await refreshProducts();
  document.getElementById('alternateBarcodeValue').focus();
 }catch(err){status.textContent='تعذرت إضافة الباركود البديل: '+err;}
 finally{button.disabled=false;}
});
onElement('cancelProductEdit','click',()=>document.getElementById('editProductOverlay').hidden=true);
onElement('editProductForm','submit',async e=>{e.preventDefault();const status=document.getElementById('editProductStatus');const button=document.getElementById('saveProductChanges');button.disabled=true;status.className='status';status.textContent=currentLanguage==='en'?'Saving changes…':'جارٍ حفظ التعديلات…';try{const p=await invoke('update_product',{id:Number(document.getElementById('editId').value),code:document.getElementById('editCode').value.trim(),name:document.getElementById('editName').value.trim(),unit:document.getElementById('editUnit').value.trim(),barcode:document.getElementById('editBarcode').value.trim(),purchasePriceMinor:Math.round(Number(document.getElementById('editPurchase').value)*100),salePriceMinor:Math.round(Number(document.getElementById('editSale').value)*100)});status.className='status status-success';status.textContent=currentLanguage==='en'?`Product “${p.name}” saved. Primary barcode: ${p.barcode}. Previous barcode retained as an alternate when changed.`:`تم تعديل «${p.name}». الباركود الأساسي الحالي: ${p.barcode}. تم الحفاظ على الباركود السابق إذا تم تغييره.`;await refreshProducts();setTimeout(()=>{document.getElementById('editProductOverlay').hidden=true;},900);}catch(err){status.className='status status-error';const raw=String(err?.message||err);status.textContent=(currentLanguage==='en'?'Could not save product: ':'فشل حفظ التعديلات: ')+(currentLanguage==='en'?localizeText(raw):raw);console.error('[Madar] update_product failed:',err);}finally{button.disabled=false;}});
// Clipboard paste is handled by the custom context menu and Rust clipboard bridge.
['barcode','editBarcode','alternateBarcodeValue','invoiceBarcodeScan'].forEach(id=>{const field=document.getElementById(id);if(field)field.maxLength=128;});
window.addEventListener('error',event=>{console.error('[Madar renderer error]',event.message,event.filename,event.lineno,event.error);});
window.addEventListener('unhandledrejection',event=>{console.error('[Madar unhandled promise rejection]',event.reason);});
onElement('productSearch','input',e=>refreshProducts(e.target.value));

onElement('refresh','click',()=>refreshProducts());
// Stock balances are an immutable movement ledger; balances are never edited directly.
let stockRowsCache=[]; // full stock-balance cache used by the dashboard and movement form
let stockProductChoices=[];
const quantityText=n=>Math.abs(Number(n||0)/1000).toLocaleString(currentLanguage==='en'?'en-US':'ar-EG',{maximumFractionDigits:3});
const quantityBalanceText=n=>(Number(n||0)/1000).toLocaleString(currentLanguage==='en'?'en-US':'ar-EG',{maximumFractionDigits:3});
function renderStockBalances(rows){
 const body=document.getElementById('stockBalances');
 body.innerHTML=rows.map(p=>`<tr><td>${escapeHtml(p.code)}</td><td>${escapeHtml(p.name)}</td><td>${escapeHtml(p.unit)}</td><td><code>${escapeHtml(p.barcode)}</code></td><td>${quantityBalanceText(p.quantity_milli)}</td><td>${p.is_active?'نشط':'موقوف'}</td></tr>`).join('')||'<tr><td colspan="6" class="muted">لا توجد أصناف تطابق البحث.</td></tr>';
 const activeWithStock=stockRowsCache.filter(p=>p.is_active&&Number(p.quantity_milli)>0).length;
 const stat=document.getElementById('dashboardInStockProducts');
 if(stat)stat.textContent=activeWithStock.toLocaleString(currentLanguage==='en'?'en-US':'ar-EG');
}
async function loadStockMovementProducts(){
 if(!invoke)return;
 stockProductChoices=await invoke('list_products');
 const select=document.getElementById('stockMovementProduct');
 const previous=select.value;
 select.innerHTML='<option value="">اختر الصنف</option>'+stockProductChoices.map(p=>`<option value="${p.id}">${escapeHtml(p.name)} — ${escapeHtml(p.code)} — ${escapeHtml(p.barcode)}</option>`).join('');
 if(previous&&stockProductChoices.some(p=>String(p.id)===previous))select.value=previous;
 updateStockAvailableBalance();
}
async function loadStockMovementHistory(){
 if(!invoke)return;
 const rows=await invoke('list_stock_movements');
 document.getElementById('stockMovements').innerHTML=rows.map(m=>{
  const inbound=Number(m.quantity_milli)>0;
  return `<tr><td>${escapeHtml(m.created_at)}</td><td>${escapeHtml(m.product_code)}</td><td>${escapeHtml(m.product_name)}</td><td>${inbound?'وارد':'منصرف'}</td><td>${inbound?'+':'−'}${quantityText(m.quantity_milli)}</td><td>${escapeHtml(m.unit)}</td><td>${escapeHtml(m.notes)}</td></tr>`;
 }).join('')||'<tr><td colspan="7" class="muted">لم تُسجّل حركات مخزون بعد.</td></tr>';
}
async function refreshStock(query,includeSecondary=false){
 const listStatus=document.getElementById('stockListStatus');
 if(!invoke){listStatus.textContent='عرض المخزون متاح عند تشغيل تطبيق مدار داخل Tauri.';return;}
 try{
  const term=query??document.getElementById('stockSearch').value;
  const displayedRows=await invoke('list_stock',{query:term});
  if(includeSecondary||stockRowsCache.length===0){
   stockRowsCache=term?await invoke('list_stock',{query:''}):displayedRows;
   await Promise.all([loadStockMovementProducts(),loadStockMovementHistory()]);
  }else if(!term){
   stockRowsCache=displayedRows;
  }
  renderStockBalances(displayedRows);
  listStatus.textContent=`عدد الأصناف المعروضة: ${displayedRows.length.toLocaleString(currentLanguage==='en'?'en-US':'ar-EG')}`;
 }catch(err){listStatus.textContent='تعذر تحميل المخزون: '+err;}
}
function updateStockAvailableBalance(){
 const id=Number(document.getElementById('stockMovementProduct').value);
 const p=stockRowsCache.find(row=>Number(row.product_id)===id);
 document.getElementById('stockAvailableBalance').textContent=p
  ?`الرصيد الحالي للصنف «${p.name}»: ${quantityBalanceText(p.quantity_milli)} ${p.unit}${p.is_active?'':' — الصنف موقوف ولا يمكن تسجيل حركة له.'}`
  :'اختر صنفًا لعرض رصيده الحالي.';
}
async function resolveStockScanner(){
 const input=document.getElementById('stockScanner');
 const value=input.value.trim();
 if(!value)return;
 const status=document.getElementById('stockMovementStatus');
 if(!invoke){status.textContent='قراءة الباركود متاحة عند تشغيل التطبيق داخل Tauri.';return;}
 try{
  let p=await invoke('find_product_by_barcode',{barcode:value});
  if(!p){const rows=await invoke('search_products',{query:value});p=rows.find(row=>row.code===value)||(rows.length===1?rows[0]:null);}
  if(!p){status.textContent='لم يتم العثور على صنف مطابق للباركود أو الكود. تأكد أن الصنف نشط.';return;}
  const select=document.getElementById('stockMovementProduct');
  if(!stockProductChoices.some(row=>Number(row.id)===Number(p.id)))await loadStockMovementProducts();
  select.value=String(p.id);updateStockAvailableBalance();
  status.textContent=`تم تحديد الصنف: ${p.name} (${p.code}).`;
  document.getElementById('stockMovementQuantity').focus();
 }catch(err){status.textContent='تعذر قراءة الصنف: '+err;}
}
onElement('stockScanner','keydown',event=>{if(event.key==='Enter'){event.preventDefault();resolveStockScanner();}});
onElement('stockMovementProduct','change',updateStockAvailableBalance);
let stockSearchTimer;
onElement('stockSearch','input',event=>{clearTimeout(stockSearchTimer);const value=event.target.value;stockSearchTimer=setTimeout(()=>refreshStock(value,false),120);});
onElement('refreshStockBtn','click',()=>refreshStock(undefined,true));
onElement('clearStockMovement','click',()=>{
 document.getElementById('stockScanner').value='';document.getElementById('stockMovementQuantity').value='';document.getElementById('stockMovementNotes').value='';
 document.getElementById('stockMovementDirection').value='in';document.getElementById('stockMovementProduct').value='';
 document.getElementById('stockMovementStatus').textContent='';updateStockAvailableBalance();document.getElementById('stockScanner').focus();
});
onElement('stockMovementForm','submit',async event=>{
 event.preventDefault();
 const status=document.getElementById('stockMovementStatus');const button=document.getElementById('saveStockMovement');
 if(!invoke){status.textContent='تسجيل حركات المخزون متاح عند تشغيل تطبيق مدار داخل Tauri.';return;}
 const productId=Number(document.getElementById('stockMovementProduct').value);
 const quantity=Number(document.getElementById('stockMovementQuantity').value);
 const direction=document.getElementById('stockMovementDirection').value;
 if(!productId){status.textContent='اختر الصنف أولًا، ويمكنك مسح باركوده في الحقل المخصص.';return;}
 if(!Number.isFinite(quantity)||quantity<=0){status.textContent='أدخل كمية أكبر من صفر.';return;}
 button.disabled=true;status.textContent='جارٍ تسجيل حركة المخزون...';
 try{
  const result=await invoke('record_stock_movement',{productId,direction,quantityMilli:Math.round(quantity*1000),notes:document.getElementById('stockMovementNotes').value.trim()||null});
  status.textContent=result;
  document.getElementById('stockMovementQuantity').value='';document.getElementById('stockMovementNotes').value='';document.getElementById('stockScanner').value='';
  await refreshStock(undefined,true);updateStockAvailableBalance();
 }catch(err){status.textContent='تعذر تسجيل حركة المخزون: '+err;}
 finally{button.disabled=false;}
});

let backupStatus=document.getElementById('backupStatus');
let restorePointsEl=document.getElementById('restorePoints');
const dialog=window.MadarDialogs;
function confirmRestore(message){
 return new Promise(resolve=>{
  const overlay=document.getElementById('restoreConfirmOverlay');
  const ok=document.getElementById('restoreConfirmOk');
  const cancel=document.getElementById('restoreConfirmCancel');
  document.getElementById('restoreConfirmMessage').textContent=message;
  overlay.hidden=false;
  const finish=value=>{overlay.hidden=true;ok.removeEventListener('click',onOk);cancel.removeEventListener('click',onCancel);resolve(value);};
  const onOk=()=>finish(true);
  const onCancel=()=>finish(false);
  ok.addEventListener('click',onOk,{once:true});
  cancel.addEventListener('click',onCancel,{once:true});
 });
}
async function refreshRestorePoints(){
 if(!invoke){restorePointsEl.innerHTML='<div class="restore-empty muted">قائمة نقاط الاستعادة تظهر عند تشغيل تطبيق مدار.</div>';return;}
 try{
  const points=await invoke('list_restore_points');
  if(!points.length){restorePointsEl.innerHTML='<div class="restore-empty muted">لا توجد نقاط استعادة تلقائية حتى الآن. عند تنفيذ أول استعادة، سيحفظ مدار الوضع الحالي هنا تلقائيًا.</div>';return;}
  restorePointsEl.innerHTML=points.map((p,i)=>`<div class="restore-item"><div class="restore-meta"><strong>${i===0?'أحدث نقطة استعادة':'نقطة استعادة '+(i+1)}</strong><span class="muted">${new Date(Number(p.modified_at_secs)*1000).toLocaleString(currentLanguage==='en'?'en-US':'ar-EG')}</span><span class="muted">حجم النسخة: ${(Number(p.size_bytes)/1024/1024).toFixed(2)} ميجابايت</span><code>${escapeHtml(p.file_name)}</code></div><div class="actions" style="margin-top:0"><button class="btn danger restore-point-btn" data-permission="backup.restore" data-path="${escapeHtml(p.path)}" type="button">استعادة هذه النسخة</button><button class="btn secondary delete-point-btn" data-permission="backup.points.delete" data-path="${escapeHtml(p.path)}" data-name="${escapeHtml(p.file_name)}" type="button">حذف النسخة</button></div></div>`).join('');
  applyElementPermissions();
  restorePointsEl.querySelectorAll('.delete-point-btn').forEach(btn=>btn.addEventListener('click',async()=>{
   const path=btn.dataset.path;const name=btn.dataset.name;
   if(!(await confirmRestore(`هل تريد حذف نقطة الاستعادة التالية نهائيًا؟\n${name}\n\nلن يمكن التراجع عن الحذف.`))){backupStatus.textContent='تم إلغاء حذف نقطة الاستعادة.';return;}
   btn.disabled=true;backupStatus.textContent='جارٍ حذف نقطة الاستعادة...';
   try{backupStatus.textContent=await invoke('delete_restore_point',{pointPath:path});await refreshRestorePoints();}
   catch(err){backupStatus.textContent='فشل حذف نقطة الاستعادة: '+err;btn.disabled=false;}
  }));
  restorePointsEl.querySelectorAll('.restore-point-btn').forEach(btn=>btn.addEventListener('click',async()=>{
   const path=btn.dataset.path;
   if(!(await confirmRestore('سيتم استبدال البيانات الحالية بمحتويات نقطة الاستعادة المحددة. بعد الضغط على موافق فقط، سيحفظ مدار الوضع الحالي كنقطة أمان ثم يبدأ الاستعادة. هل تريد المتابعة؟'))) {backupStatus.textContent='تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.';return;}
   btn.disabled=true;backupStatus.textContent='جارٍ التحقق من نقطة الاستعادة وتطبيقها...';
   try{const result=await invoke('restore_backup',{backupPath:path});backupStatus.textContent=result+'\nتم تحديث البيانات.';if(result.includes('سجّل الدخول مرة أخرى')){currentUser=null;document.getElementById('currentUserBadge').hidden=true;document.getElementById('logoutBtn').hidden=true;document.getElementById('changePasswordBtn').hidden=true;closeHeaderMenus();renderHeaderUser(null);renderHeaderCompany(null);await 

initializeAuth();return;}if(currentUser?.permissions.includes('products.view'))await refreshProducts();if(currentUser?.permissions.includes('backup.view_points'))await refreshRestorePoints();}
   catch(err){backupStatus.textContent='فشلت الاستعادة: '+err;btn.disabled=false;}
  }));
 }catch(err){restorePointsEl.innerHTML='<div class="restore-empty muted">تعذر تحميل نقاط الاستعادة: '+escapeHtml(err)+'</div>';}
}
onElement('refreshRestorePoints','click',refreshRestorePoints);
function timestampForFile(){const d=new Date();const pad=n=>String(n).padStart(2,'0');return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}_${pad(d.getHours())}-${pad(d.getMinutes())}-${pad(d.getSeconds())}`;}
onElement('backupBtn','click',async()=>{
 if(!invoke||!dialog?.save){backupStatus.textContent='ميزة اختيار الملفات متاحة عند تشغيل نسخة Tauri المبنية من المشروع.';return;}
 try{const destination=await dialog.save({title:'حفظ نسخة احتياطية من مدار',defaultPath:`Madar-Backup-${timestampForFile()}.db`,filters:[{name:'قاعدة بيانات SQLite',extensions:['db','sqlite']}]});if(!destination){backupStatus.textContent='تم إلغاء إنشاء النسخة الاحتياطية.';return;}backupStatus.textContent='جارٍ إنشاء النسخة الاحتياطية والتحقق منها...';const saved=await invoke('create_backup',{destination});backupStatus.textContent=`تم إنشاء النسخة الاحتياطية والتحقق من سلامتها:
${saved}`;}catch(err){backupStatus.textContent='فشل إنشاء النسخة الاحتياطية: '+err;}
});
onElement('restoreBtn','click',async()=>{
 if(!invoke||!dialog?.open){backupStatus.textContent='ميزة اختيار الملفات متاحة عند تشغيل نسخة Tauri المبنية من المشروع.';return;}
 try{const selected=await dialog.open({title:'اختيار نسخة مدار للاستعادة',multiple:false,directory:false,filters:[{name:'قاعدة بيانات SQLite',extensions:['db','sqlite']}]});if(!selected){backupStatus.textContent='تم إلغاء الاستعادة.';return;}if(!(await confirmRestore('تحذير: ستُستبدل البيانات الحالية بمحتويات النسخة المختارة. بعد الضغط على موافق فقط، سيحتفظ مدار بنسخة أمان من قاعدة البيانات الحالية ثم يبدأ الاستعادة. هل تريد المتابعة؟'))){backupStatus.textContent='تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.';return;}backupStatus.textContent='جارٍ التحقق من النسخة واستعادة البيانات...';const result=await invoke('restore_backup',{backupPath:selected});backupStatus.textContent=result+'\nسيُعاد تحميل قائمة الأصناف الآن.';if(result.includes('سجّل الدخول مرة أخرى')){currentUser=null;document.getElementById('currentUserBadge').hidden=true;document.getElementById('logoutBtn').hidden=true;document.getElementById('changePasswordBtn').hidden=true;closeHeaderMenus();renderHeaderUser(null);renderHeaderCompany(null);await initializeAuth();return;}if(currentUser?.permissions.includes('products.view'))await refreshProducts();if(currentUser?.permissions.includes('backup.view_points'))await refreshRestorePoints();}catch(err){backupStatus.textContent='فشلت الاستعادة: '+err;}
});
// Stage 1: customer and supplier directory.
let activePartyType='customer';
let partyRowsCache=[];
function partyLabel(type){return type==='customer'?'العميل':'المورد';}
function openPartyEditor(row=null){
 const allowed=currentUser?.permissions.includes(activePartyType==='customer'?(row?'customers.edit':'customers.create'):(row?'suppliers.edit':'suppliers.create'));
 if(!allowed){document.getElementById('partiesStatus').textContent='ليس لديك صلاحية لتنفيذ هذه العملية.';return;}
 const form=document.getElementById('partyEditorForm');form.reset();
 document.getElementById('partyId').value=row?.id??'';document.getElementById('partyCode').value=row?.code??'';document.getElementById('partyName').value=row?.name??'';document.getElementById('partyEmail').value=row?.email??'';document.getElementById('partyPhone').value=row?.phone??'';document.getElementById('partyAddress').value=row?.address??'';document.getElementById('partyNotes').value=row?.notes??'';
 document.getElementById('partyEditorTitle').textContent=row?`تعديل ${partyLabel(activePartyType)}`:`إضافة ${partyLabel(activePartyType)}`;document.getElementById('partyEditorStatus').textContent='';document.getElementById('partyEditorOverlay').hidden=false;document.getElementById('partyCode').focus();
}
async function loadParties(){
 const status=document.getElementById('partiesStatus');const body=document.getElementById('partiesTable');
 if(!invoke||!currentUser)return;
 const viewPermission=activePartyType==='customer'?'customers.view':'suppliers.view';
 if(!currentUser.permissions.includes(viewPermission)){activePartyType=activePartyType==='customer'?'supplier':'customer';}
 document.getElementById('customersTab').classList.toggle('secondary',activePartyType!=='customer');document.getElementById('suppliersTab').classList.toggle('secondary',activePartyType!=='supplier');
 document.getElementById('partiesSectionTitle').textContent=activePartyType==='customer'?'العملاء':'الموردون';
 const addBtn=document.getElementById('addPartyBtn');const canCreate=currentUser.permissions.includes(activePartyType==='customer'?'customers.create':'suppliers.create');addBtn.hidden=!canCreate;addBtn.textContent=activePartyType==='customer'?'＋ إضافة عميل':'＋ إضافة مورد';
 try{
  partyRowsCache=await invoke('list_parties',{entityType:activePartyType,query:document.getElementById('partySearch').value||null});
  body.innerHTML=partyRowsCache.map(r=>`<tr><td>${escapeHtml(r.code)}</td><td>${escapeHtml(r.name)}</td><td>${escapeHtml(r.email||'—')}</td><td>${escapeHtml(r.phone||'—')}</td><td>${escapeHtml(r.address||'—')}</td><td><span class="party-state">${r.isActive?'نشط':'موقوف'}</span></td><td><div class="table-actions"><button type="button" class="btn secondary small-btn party-edit" data-id="${r.id}" data-permission="${activePartyType==='customer'?'customers.edit':'suppliers.edit'}">تعديل</button><button type="button" class="btn ${r.isActive?'danger':'secondary'} small-btn party-toggle" data-id="${r.id}" data-active="${r.isActive?'false':'true'}" data-permission="${activePartyType==='customer'?'customers.status.manage':'suppliers.status.manage'}">${r.isActive?'إيقاف':'تفعيل'}</button></div></td></tr>`).join('')||`<tr><td colspan="7" class="muted">لا توجد سجلات مطابقة.</td></tr>`;
  status.textContent=`عدد السجلات: ${partyRowsCache.length}`;applyElementPermissions();
  body.querySelectorAll('.party-edit').forEach(btn=>btn.addEventListener('click',()=>openPartyEditor(partyRowsCache.find(r=>Number(r.id)===Number(btn.dataset.id)))));
  body.querySelectorAll('.party-toggle').forEach(btn=>btn.addEventListener('click',async()=>{
   const row=partyRowsCache.find(r=>Number(r.id)===Number(btn.dataset.id));if(!row)return;const active=btn.dataset.active==='true';
   if(!(await confirmProductAction(`${active?'هل تريد تفعيل':'هل تريد إيقاف'} ${partyLabel(activePartyType)} «${row.name}»؟\nلن يتم حذف البيانات أو الفواتير المرتبطة.`,`${active?'تفعيل':'إيقاف'} ${partyLabel(activePartyType)}`,active?'نعم، فعّل':'نعم، أوقف')))return;
   btn.disabled=true;try{const message=await invoke('set_party_active',{entityType:activePartyType,id:Number(row.id),active});status.textContent=message;await loadParties();await loadAuditLog('');}catch(err){status.textContent='تعذر تغيير حالة السجل: '+err;btn.disabled=false;}
  }));
 }catch(err){status.textContent='تعذر تحميل السجلات: '+err;body.innerHTML='<tr><td colspan="7" class="muted">تعذر تحميل البيانات.</td></tr>';}
}
onElement('customersTab','click',()=>{activePartyType='customer';document.getElementById('partySearch').value='';loadParties();});
onElement('suppliersTab','click',()=>{activePartyType='supplier';document.getElementById('partySearch').value='';loadParties();});
onElement('addPartyBtn','click',()=>openPartyEditor());
onElement('refreshPartiesBtn','click',loadParties);
onElement('cancelPartyEditor','click',()=>{document.getElementById('partyEditorOverlay').hidden=true;});
onElement('partySearch','input',()=>{clearTimeout(window.__partySearchTimer);window.__partySearchTimer=setTimeout(loadParties,150);});
onElement('partyEditorForm','submit',async e=>{
 e.preventDefault();const btn=document.getElementById('savePartyBtn');const status=document.getElementById('partyEditorStatus');const rawId=document.getElementById('partyId').value;btn.disabled=true;
 try{const message=await invoke('save_party',{entityType:activePartyType,id:rawId?Number(rawId):null,code:document.getElementById('partyCode').value,name:document.getElementById('partyName').value,email:document.getElementById('partyEmail').value.trim()||null,phone:document.getElementById('partyPhone').value.trim()||null,address:document.getElementById('partyAddress').value.trim()||null,notes:document.getElementById('partyNotes').value.trim()||null});status.textContent=message;await loadParties();await loadAuditLog('');document.getElementById('partyEditorOverlay').hidden=true;}
 catch(err){status.textContent='تعذر حفظ السجل: '+err;}finally{btn.disabled=false;}
});

// Stage 2 and 3: draft-first invoices with atomic stock posting and reversal.
let invoiceProductsCache=[];let currentInvoiceType='purchase';let editingInvoiceId=null;let currentInvoiceTaxRateBps=0;let currentInvoicePricesIncludeTax=false;
function invoicePageStatus(type){return document.getElementById(type==='purchase'?'purchaseInvoiceStatus':'saleInvoiceStatus');}
function invoiceTableBody(type){return document.getElementById(type==='purchase'?'purchaseInvoicesTable':'saleInvoicesTable');}
function invoiceTypeLabel(type){return type==='purchase'?'فاتورة مشتريات':'فاتورة مبيعات';}
function setInvoiceStatus(el,message,ok=false){el.textContent=message;el.classList.toggle('status-success',!!ok);el.classList.toggle('status-error',!ok&&!!message);}
function invoiceStatusLabel(status){return ({draft:'مسودة',posted:'معتمدة',cancelled:'ملغاة'})[status]||status;}
let currentCurrency='EGP';function formatInvoiceMoney(value){return money(value)+' '+(currentLanguage==='en'?currentCurrency:(currentCurrency==='EGP'?'جنيه':currentCurrency));}
let activePrintableDoc={type:'sale',id:null};
function printCurrentDetails(){if(!activePrintableDoc.id)return;return activePrintableDoc.type==='quote'?printQuotationDocument(activePrintableDoc.id):printInvoiceDocument(activePrintableDoc.type,activePrintableDoc.id);}
onElement('printCurrentCommercialDoc','click',printCurrentDetails);
function fmtQuantity(milli){return (Number(milli||0)/1000).toLocaleString(currentLanguage==='en'?'en-US':'ar-EG',{maximumFractionDigits:3});}
function printFrame(html){
 const frame=document.createElement('iframe');frame.setAttribute('aria-hidden','true');frame.style.cssText='position:fixed;right:0;bottom:0;width:1px;height:1px;border:0;opacity:0;pointer-events:none';document.body.appendChild(frame);
 const doc=frame.contentDocument||frame.contentWindow.document;doc.open();doc.write(html);doc.close();
 setTimeout(()=>{try{frame.contentWindow.focus();frame.contentWindow.print();}catch(err){statusEl.textContent=(currentLanguage==='en'?'Could not open print dialog: ':'تعذر فتح نافذة الطباعة: ')+err;}setTimeout(()=>frame.remove(),1500);},450);
}
function buildPrintableHtml({title,number,date,party,status,notes,subtotal,discount,total,taxMinor=0,taxRateBps=0,pricesIncludeTax=false,items,company,validUntil,paidMinor}){
 const en=currentLanguage==='en';const c=company||{};const companyName=c.name|| (en?'Your company name':'اسم شركتك');const currency=c.currency_code||'EGP';
 const text={document:en?'DOCUMENT':'مستند',number:en?'Document no.':'رقم المستند',date:en?'Date':'التاريخ',valid:en?'Valid until':'صالح حتى',party:en?'Party':'الطرف',status:en?'Status':'الحالة',code:en?'Code':'الكود',item:en?'Description':'الصنف',unit:en?'Unit':'الوحدة',qty:en?'Quantity':'الكمية',unitPrice:en?'Unit price':'سعر الوحدة',lineTotal:en?'Amount':'الإجمالي',subtotal:en?'Subtotal':'الإجمالي قبل الخصم',discount:en?'Discount':'الخصم',total:en?'Total due':'الإجمالي النهائي',notes:en?'Notes':'ملاحظات',prepared:en?'Prepared by':'أعدّها',signature:en?'Authorized signature':'توقيع المسؤول',customerSign:en?'Customer signature':'توقيع العميل',thank:en?'Thank you for your business.':'شكرًا لثقتكم بنا.',tax:en?'Tax no.':'الرقم الضريبي',registry:en?'Commercial register':'السجل التجاري',phone:en?'Phone':'الهاتف',email:en?'Email':'البريد الإلكتروني',address:en?'Address':'العنوان',taxLabel:en?'VAT':'ضريبة القيمة المضافة',taxIncluded:en?'VAT included in prices':'الضريبة المتضمنة في السعر',taxAdded:en?'VAT added':'الضريبة المضافة'};
 const details=[c.phone?`${text.phone}: ${escapeHtml(c.phone)}`:'',c.email?`${text.email}: ${escapeHtml(c.email)}`:'',c.website?escapeHtml(c.website):''].filter(Boolean).join(' · ');
 const meta=[c.tax_number?`${text.tax}: ${escapeHtml(c.tax_number)}`:'',c.commercial_register?`${text.registry}: ${escapeHtml(c.commercial_register)}`:''].filter(Boolean).join(' · ');
 const unitPriceLabel=taxRateBps>0?(pricesIncludeTax?(en?'Unit price (VAT incl.)':'سعر الوحدة شامل الضريبة'):(en?'Unit price (VAT excl.)':'سعر الوحدة غير شامل الضريبة')):text.unitPrice;
 const rows=(items||[]).map((it,i)=>`<tr><td>${i+1}</td><td>${escapeHtml(it.productCode||it.code||'')}</td><td>${escapeHtml(it.productName||it.name||'')}</td><td>${escapeHtml(it.unit||'')}</td><td class="num">${fmtQuantity(it.quantityMilli??it.quantity_milli)}</td><td class="num">${(Number((it.unitPriceMinor??it.unit_price_minor)??0)/100).toFixed(2)} ${escapeHtml(currency)}</td><td class="num">${(Number((it.lineTotalMinor??it.line_total_minor)??0)/100).toFixed(2)} ${escapeHtml(currency)}</td></tr>`).join('');
 const statusLabel={draft:en?'Draft':'مسودة',posted:en?'Posted':'معتمدة',cancelled:en?'Cancelled':'ملغاة',sent:en?'Sent':'مرسلة',accepted:en?'Accepted':'مقبولة',rejected:en?'Rejected':'مرفوضة',expired:en?'Expired':'منتهية',converted:en?'Converted to invoice':'محوّلة لفاتورة'}[status]||status||'';
 const logo=c.logo_data_url?`<img class="logo" src="${escapeHtml(c.logo_data_url)}" alt="logo">`:'';
 const paymentSummary=paidMinor===undefined||paidMinor===null?'':`<div><span>${en?'Paid':'المدفوع'}</span><strong>${(Number(paidMinor||0)/100).toFixed(2)} ${escapeHtml(currency)}</strong></div><div><span>${en?'Balance due':'المتبقي'}</span><strong>${(Math.max(0,Number(total||0)-Number(paidMinor||0))/100).toFixed(2)} ${escapeHtml(currency)}</strong></div>`;const taxRate=Math.max(0,Number(taxRateBps)||0);const taxAmount=Number(taxMinor)||0;const taxRow=taxRate>0?`<div><span>${pricesIncludeTax?text.taxIncluded:text.taxAdded} (${(taxRate/100).toFixed(2)}%)</span><strong>${(taxAmount/100).toFixed(2)} ${escapeHtml(currency)}</strong></div>`:'';
 return `<!doctype html><html lang="${en?'en':'ar'}" dir="${en?'ltr':'rtl'}"><head><meta charset="utf-8"><title>${escapeHtml(title)} ${escapeHtml(number)}</title><style>@page{size:A4;margin:13mm}*{box-sizing:border-box}body{font-family:Arial,Tahoma,sans-serif;color:#16202b;margin:0;font-size:12px}header{display:flex;align-items:center;justify-content:space-between;border-bottom:3px solid #1e78c8;padding-bottom:14px;margin-bottom:18px;gap:18px}.brand{display:flex;align-items:center;gap:14px}.logo{max-width:115px;max-height:75px;object-fit:contain}.company h1{font-size:21px;margin:0 0 6px;color:#174b7c}.company div{line-height:1.7;color:#485665}.docTitle{text-align:${en?'right':'left'};min-width:180px}.docTitle h2{font-size:22px;margin:0 0 8px;color:#1e78c8}.pill{display:inline-block;border:1px solid #cbd5e1;border-radius:18px;padding:4px 9px;color:#475569}.meta{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin:15px 0 20px}.meta div{background:#f4f7fb;border:1px solid #e1e8f0;border-radius:6px;padding:9px}.meta b{display:block;color:#526171;font-size:10px;margin-bottom:5px}.meta span{font-weight:600}.party{border-inline-start:3px solid #1e78c8;padding:8px 12px;margin:12px 0 18px;background:#fbfdff}table{width:100%;border-collapse:collapse}thead{display:table-header-group}th{background:#edf3f8;color:#23384b;text-align:${en?'left':'right'};padding:9px;border:1px solid #d6e0ea}td{padding:9px;border:1px solid #dce4ec}tbody tr:nth-child(even){background:#fafcfe}.num{white-space:nowrap;text-align:${en?'right':'left'};font-variant-numeric:tabular-nums}.totals{width:min(360px,100%);margin:16px 0 0 auto;${en?'margin-left:auto;margin-right:0':'margin-right:auto;margin-left:0'}}.totals div{display:flex;justify-content:space-between;padding:7px 10px;border-bottom:1px solid #e2e8f0}.totals .grand{background:#edf6ff;font-weight:bold;font-size:15px;color:#174b7c;border:1px solid #c5dff8}.notes{white-space:pre-wrap;margin-top:20px;padding:12px;border:1px solid #e2e8f0;border-radius:6px;min-height:38px}.signatures{display:grid;grid-template-columns:1fr 1fr;gap:55px;margin-top:55px}.signatures div{text-align:center;border-top:1px solid #8895a5;padding-top:8px;color:#526171}.footer{margin-top:30px;padding-top:10px;border-top:1px solid #d9e2eb;text-align:center;color:#586777;font-size:11px;line-height:1.7}.fine{font-size:10px;color:#657587;margin-top:4px}@media print{body{-webkit-print-color-adjust:exact;print-color-adjust:exact}.no-print{display:none}}</style></head><body><header><div class="brand">${logo}<div class="company"><h1>${escapeHtml(companyName)}</h1><div>${c.address?`${text.address}: ${escapeHtml(c.address)}<br>`:''}${details}${meta?`<br>${meta}`:''}</div></div></div><div class="docTitle"><h2>${escapeHtml(title)}</h2><span class="pill">${escapeHtml(statusLabel)}</span></div></header><section class="meta"><div><b>${text.number}</b><span>${escapeHtml(number)}</span></div><div><b>${text.date}</b><span>${escapeHtml(date||'')}</span></div>${validUntil?`<div><b>${text.valid}</b><span>${escapeHtml(validUntil)}</span></div>`:`<div><b>${text.document}</b><span>${escapeHtml(title)}</span></div>`}</section><section class="party"><b>${text.party}: </b>${escapeHtml(party||'—')}</section><table><thead><tr><th>#</th><th>${text.code}</th><th>${text.item}</th><th>${text.unit}</th><th class="num">${text.qty}</th><th class="num">${unitPriceLabel}</th><th class="num">${text.lineTotal}</th></tr></thead><tbody>${rows}</tbody></table><section class="totals"><div><span>${text.subtotal}</span><strong>${(Number(subtotal||0)/100).toFixed(2)} ${escapeHtml(currency)}</strong></div><div><span>${text.discount}</span><strong>${(Number(discount||0)/100).toFixed(2)} ${escapeHtml(currency)}</strong></div>${taxRow}${paymentSummary}<div class="grand"><span>${text.total}</span><strong>${(Number(total||0)/100).toFixed(2)} ${escapeHtml(currency)}</strong></div></section>${notes?`<section class="notes"><b>${text.notes}: </b>${escapeHtml(notes)}</section>`:''}<section class="signatures"><div>${text.signature}</div><div>${text.customerSign}</div></section><footer class="footer">${escapeHtml(c.invoice_footer||text.thank)}<div class="fine">${escapeHtml(companyName)} · ${escapeHtml(number)}</div></footer></body></html>`;
}
async function printInvoiceDocument(type,id){try{const [details,company]=await Promise.all([invoke('get_invoice_details',{invoiceType:type,invoiceId:id}),invoke('get_company_printing_profile',{documentType:type})]);const v=details.invoice;const title=currentLanguage==='en'?(type==='purchase'?'PURCHASE INVOICE':'SALES INVOICE'):(type==='purchase'?'فاتورة مشتريات':'فاتورة مبيعات');const html=buildPrintableHtml({title,number:v.invoiceNo,date:v.invoiceDate,party:v.partyName,status:v.status,notes:v.notes,subtotal:v.subtotalMinor,discount:v.discountMinor,total:v.totalMinor,taxMinor:v.taxMinor??0,taxRateBps:v.taxRateBps??0,pricesIncludeTax:Boolean(v.pricesIncludeTax),paidMinor:v.paidMinor??0,items:details.items,company});printFrame(html);}catch(err){statusEl.textContent=(currentLanguage==='en'?'Could not prepare invoice for printing: ':'تعذر تجهيز الفاتورة للطباعة: ')+err;}}
async function printQuotationDocument(id){try{const [details,company]=await Promise.all([invoke('get_quotation_details',{quotationId:id}),invoke('get_company_printing_profile',{documentType:'quote'})]);const v=details.quotation;const html=buildPrintableHtml({title:currentLanguage==='en'?'QUOTATION':'عرض سعر',number:v.quoteNo,date:v.quoteDate,validUntil:v.validUntil,party:v.customerName,status:v.status,notes:v.notes,subtotal:v.subtotalMinor,discount:v.discountMinor,total:v.totalMinor,taxMinor:v.taxMinor??0,taxRateBps:v.taxRateBps??0,pricesIncludeTax:Boolean(v.pricesIncludeTax),items:details.items,company});printFrame(html);}catch(err){document.getElementById('quotationsStatus').textContent=(currentLanguage==='en'?'Could not print quotation: ':'تعذر طباعة عرض السعر: ')+err;}}
async function loadInvoices(type, query){
 if(!invoke||!currentUser)return;
 const status=invoicePageStatus(type);const body=invoiceTableBody(type);const search=query??document.getElementById(type==='purchase'?'purchaseInvoiceSearch':'saleInvoiceSearch').value;
 try{
  const rows=await invoke('list_invoices',{invoiceType:type,query:search||null});
  body.innerHTML=rows.map(row=>{
   const permPrefix=type==='purchase'?'purchases':'sales';const buttons=[`<button type="button" class="btn secondary small-btn invoice-details" data-id="${row.id}">تفاصيل</button>`,`<button type="button" class="btn secondary small-btn invoice-print" data-id="${row.id}">طباعة</button>`];
   if(row.status==='draft'){buttons.push(`<button type="button" class="btn secondary small-btn invoice-edit" data-id="${row.id}" data-permission="${permPrefix}.edit_draft">تعديل المسودة</button>`);buttons.push(`<button type="button" class="btn small-btn invoice-post" data-id="${row.id}" data-permission="${permPrefix}.post">اعتماد</button>`);}
   if(row.status!=='cancelled')buttons.push(`<button type="button" class="btn danger small-btn invoice-cancel" data-id="${row.id}" data-status="${row.status}" data-permission="${permPrefix}.cancel">إلغاء</button>`);
   return `<tr><td><code>${escapeHtml(row.invoiceNo)}</code></td><td>${escapeHtml(row.invoiceDate)}</td><td>${escapeHtml(row.partyName)}</td><td>${row.itemCount}</td><td>${formatInvoiceMoney(row.totalMinor)}</td><td>${invoiceStatusLabel(row.status)}</td><td>${escapeHtml(row.createdByName||'—')}</td><td><div class="table-actions">${buttons.join('')}</div></td></tr>`;
  }).join('')||'<tr><td colspan="8" class="muted">لا توجد فواتير مطابقة.</td></tr>';
  status.textContent=`عدد الفواتير المعروضة: ${rows.length}`;applyElementPermissions();
  body.querySelectorAll('.invoice-details').forEach(btn=>btn.addEventListener('click',()=>showInvoiceDetails(type,Number(btn.dataset.id))));
  body.querySelectorAll('.invoice-print').forEach(btn=>btn.addEventListener('click',()=>printInvoiceDocument(type,Number(btn.dataset.id))));
  body.querySelectorAll('.invoice-edit').forEach(btn=>btn.addEventListener('click',()=>openInvoiceEditor(type,Number(btn.dataset.id))));
  body.querySelectorAll('.invoice-post').forEach(btn=>btn.addEventListener('click',async()=>{
   const row=rows.find(r=>Number(r.id)===Number(btn.dataset.id));if(!row)return;
   if(!(await confirmProductAction(`سيتم اعتماد الفاتورة ${row.invoiceNo}. ${type==='purchase'?'سيضيف ذلك الكميات إلى المخزون.':'سيتم التحقق من المخزون وخصم الكميات.'}\nلا يمكن تعديل الفاتورة بعد الاعتماد؛ الإلغاء سيُسجل كحركة عكسية.`, 'تأكيد اعتماد الفاتورة','اعتماد الفاتورة')))return;
   btn.disabled=true;try{const result=await invoke('post_invoice',{invoiceType:type,invoiceId:Number(row.id)});status.textContent=result;await loadInvoices(type);await loadAuditLog('');}
   catch(err){status.textContent='تعذر اعتماد الفاتورة: '+err;btn.disabled=false;}
  }));
  body.querySelectorAll('.invoice-cancel').forEach(btn=>btn.addEventListener('click',async()=>{
   const row=rows.find(r=>Number(r.id)===Number(btn.dataset.id));if(!row)return;
   const warning=row.status==='posted'?(type==='purchase'?'سيحاول النظام عكس الكميات الواردة، وقد يرفض الإلغاء إذا كانت الكمية لم تعد متاحة بالمخزون.':'سيعيد النظام كميات الفاتورة إلى المخزون.'):'سيتم إلغاء المسودة دون تغيير المخزون.';
   if(!(await confirmProductAction(`هل تريد إلغاء الفاتورة ${row.invoiceNo}؟\n${warning}\nسيظل سجل الفاتورة محفوظًا.`, 'تأكيد إلغاء الفاتورة','إلغاء الفاتورة')))return;
   btn.disabled=true;try{const result=await invoke('cancel_invoice',{invoiceType:type,invoiceId:Number(row.id)});status.textContent=result;await loadInvoices(type);await loadAuditLog('');}
   catch(err){status.textContent='تعذر إلغاء الفاتورة: '+err;btn.disabled=false;}
  }));
 }catch(err){status.textContent='تعذر تحميل الفواتير: '+err;body.innerHTML='<tr><td colspan="8" class="muted">تعذر تحميل الفواتير.</td></tr>';}
}
async function openInvoiceEditor(type, invoiceId=null){
 currentInvoiceType=type;editingInvoiceId=invoiceId;const status=document.getElementById('invoiceEditorStatus');const form=document.getElementById('invoiceEditorForm');form.reset();document.getElementById('invoiceType').value=type;status.textContent='';
 const isQuote=type==='quote';document.getElementById('invoiceEditorTitle').textContent=isQuote?(invoiceId?'تعديل عرض سعر':'عرض سعر جديد'):(invoiceId?`تعديل مسودة ${type==='purchase'?'المشتريات':'المبيعات'}`:(type==='purchase'?'فاتورة مشتريات جديدة':'فاتورة مبيعات جديدة'));document.getElementById('invoiceEditorHelp').textContent=isQuote?'عرض السعر لا يغيّر المخزون. يمكن طباعته وإرساله للعميل، ثم قبوله وتحويله إلى مسودة مبيعات.':'المسودة لا تغير المخزون إلا عند اعتمادها. بعد الاعتماد لا يمكن تعديلها، ويجب الإلغاء لعكس أثرها.';document.getElementById('saveInvoiceDraftBtn').textContent=isQuote?(invoiceId?'حفظ التعديلات':'حفظ عرض السعر'):(invoiceId?'حفظ التعديلات':'حفظ المسودة');document.getElementById('quoteValidUntilLabel').hidden=!isQuote;document.getElementById('quoteValidUntil').value='';const invoiceDateLabel=document.getElementById('invoiceDate').closest('label');if(invoiceDateLabel){const dateText=invoiceDateLabel.firstChild;dateText.textContent=isQuote?'تاريخ العرض':'تاريخ الفاتورة';dateText.__madarOriginalText=dateText.textContent;if(currentLanguage==='en')dateText.textContent=localizeText(dateText.__madarOriginalText);}
 document.getElementById('invoicePartyLabel').hidden=false;const partyLabelNode=document.getElementById('invoicePartyLabel').firstChild;partyLabelNode.textContent=type==='purchase'?'المورد':'العميل';partyLabelNode.__madarOriginalText=partyLabelNode.textContent;if(currentLanguage==='en')partyLabelNode.textContent=localizeText(partyLabelNode.__madarOriginalText);
 const now=new Date();document.getElementById('invoiceDate').value=`${now.getFullYear()}-${String(now.getMonth()+1).padStart(2,'0')}-${String(now.getDate()).padStart(2,'0')}`;document.getElementById('invoiceDiscount').value='0';document.getElementById('invoiceNotes').value='';
 try{
  const taxProfile=await invoke('get_company_printing_profile',{documentType:isQuote?'quote':type});currentInvoiceTaxRateBps=Number(taxProfile.tax_rate_bps||0);currentInvoicePricesIncludeTax=Boolean(taxProfile.prices_include_tax);
  invoiceProductsCache=await invoke('list_products');
  const options=invoiceProductsCache.map(p=>`<option value="${p.id}">${escapeHtml(p.name)} — ${escapeHtml(p.code)} — ${escapeHtml(p.unit)}</option>`).join('');
  const partyType=type==='purchase'?'supplier':'customer';const parties=await invoke('list_parties',{entityType:partyType,query:null});
  const partySelect=document.getElementById('invoiceParty');partySelect.innerHTML=(type==='sale'?'<option value="">بيع نقدي بدون عميل</option>':'<option value="">اختر المورد</option>')+parties.filter(p=>p.isActive).map(p=>`<option value="${p.id}">${escapeHtml(p.name)} — ${escapeHtml(p.code)}</option>`).join('');
  document.getElementById('invoiceLinesBody').innerHTML='';
  if(invoiceId){
   const details=isQuote?await invoke('get_quotation_details',{quotationId:invoiceId}):await invoke('get_invoice_details',{invoiceType:type,invoiceId});const inv=isQuote?details.quotation:details.invoice;
   if(inv.status!=='draft')throw new Error(isQuote?'لا يمكن تعديل عرض السعر إلا إذا كان مسودة.':'لا يمكن تعديل الفاتورة إلا إذا كانت مسودة.');
   document.getElementById('invoiceDate').value=isQuote?inv.quoteDate:inv.invoiceDate;document.getElementById('invoiceParty').value=isQuote?inv.customerId:(inv.partyId??'');document.getElementById('invoiceDiscount').value=money(inv.discountMinor);document.getElementById('invoiceNotes').value=inv.notes||'';if(isQuote)document.getElementById('quoteValidUntil').value=inv.validUntil||'';
   for(const item of details.items)addInvoiceLine({productId:item.productId,quantity:Number(item.quantityMilli)/1000,unitPrice:money(item.unitPriceMinor)});
  }else addInvoiceLine();
  updateInvoiceTotals();document.getElementById('invoiceEditorOverlay').hidden=false;setTimeout(()=>document.getElementById('invoiceBarcodeScan').focus(),0);
 }catch(err){status.textContent='تعذر تحميل البيانات اللازمة للفاتورة: '+err;document.getElementById('invoiceEditorOverlay').hidden=false;}
}
function addInvoiceLine(initial={}){
 const body=document.getElementById('invoiceLinesBody');const tr=document.createElement('tr');
 const options=invoiceProductsCache.map(p=>`<option value="${p.id}" ${Number(initial.productId)===Number(p.id)?'selected':''}>${escapeHtml(p.name)} — ${escapeHtml(p.code)}</option>`).join('');
 tr.innerHTML=`<td><select class="invoice-line-product" required><option value="">اختر الصنف</option>${options}</select></td><td><input class="invoice-line-qty" type="number" min="0.001" step="0.001" value="${initial.quantity??1}" required/></td><td><input class="invoice-line-price" type="number" min="0" step="0.01" value="${initial.unitPrice??''}" required/></td><td class="invoice-line-total">0.00</td><td><button type="button" class="btn danger small-btn invoice-line-remove">حذف</button></td>`;
 const product=tr.querySelector('.invoice-line-product');const qty=tr.querySelector('.invoice-line-qty');const price=tr.querySelector('.invoice-line-price');
 product.addEventListener('change',()=>{const p=invoiceProductsCache.find(item=>Number(item.id)===Number(product.value));if(p)price.value=money(currentInvoiceType==='purchase'?p.purchase_price_minor:p.sale_price_minor);updateInvoiceTotals();});
 if(['sale','quote'].includes(currentInvoiceType)&&!currentUser.permissions.includes('sales.price.override')){price.readOnly=true;price.title='تحتاج صلاحية تجاوز سعر البيع لتغيير السعر في الفاتورة';}
 qty.addEventListener('input',updateInvoiceTotals);price.addEventListener('input',updateInvoiceTotals);
 tr.querySelector('.invoice-line-remove').addEventListener('click',()=>{tr.remove();updateInvoiceTotals();});body.appendChild(tr);updateInvoiceTotals();
}
function updateInvoiceTotals(){
 let subtotal=0;document.querySelectorAll('#invoiceLinesBody tr').forEach(tr=>{const q=Math.round(Number(tr.querySelector('.invoice-line-qty').value||0)*1000);const price=Math.round(Number(tr.querySelector('.invoice-line-price').value||0)*100);const total=Math.round(q*price/1000);subtotal+=total;tr.querySelector('.invoice-line-total').textContent=money(total);});
 const discount=Math.round(Number(document.getElementById('invoiceDiscount').value||0)*100);const base=Math.max(0,subtotal-discount);const rate=Math.max(0,Math.min(10000,Number(currentInvoiceTaxRateBps)||0));const tax=rate===0?0:(currentInvoicePricesIncludeTax?Math.round(base*rate/(10000+rate)):Math.round(base*rate/10000));const total=currentInvoicePricesIncludeTax?base:base+tax;document.getElementById('invoiceSubtotal').textContent=money(subtotal);document.getElementById('invoiceTax').textContent=`${money(tax)}${rate?` (${(rate/100).toFixed(2)}%)`:''}`;document.getElementById('invoiceTotal').textContent=money(total);
}
onElement('invoiceDiscount','input',updateInvoiceTotals);
onElement('addInvoiceLineBtn','click',()=>addInvoiceLine());
onElement('invoiceBarcodeScan','keydown',async e=>{
 if(e.key!=='Enter')return;e.preventDefault();const input=e.currentTarget;const barcode=input.value.trim();if(!barcode)return;
 const status=document.getElementById('invoiceEditorStatus');input.disabled=true;
 try{
  const product=await invoke('find_product_by_barcode',{barcode});
  if(!product){setInvoiceStatus(status,`لم يتم العثور على صنف نشط بالباركود ${barcode}.`,false);input.select();return;}
  const existing=[...document.querySelectorAll('#invoiceLinesBody tr')].find(tr=>Number(tr.querySelector('.invoice-line-product').value)===Number(product.id));
  if(existing){const q=existing.querySelector('.invoice-line-qty');q.value=(Math.round(Number(q.value||0)*1000)+1000)/1000;}
  else addInvoiceLine({productId:product.id,quantity:1,unitPrice:money(currentInvoiceType==='purchase'?product.purchase_price_minor:product.sale_price_minor)});
  updateInvoiceTotals();input.value='';setInvoiceStatus(status,`تمت إضافة ${product.name} (${product.code}) إلى الفاتورة.`,true);input.focus();
 }catch(err){setInvoiceStatus(status,'تعذر البحث عن الباركود: '+err,false);input.select();}
 finally{input.disabled=false;input.focus();}
});
onElement('openNewPurchaseInvoice','click',()=>openInvoiceEditor('purchase'));
onElement('openNewSaleInvoice','click',()=>openInvoiceEditor('sale'));
onElement('refreshPurchasesBtn','click',()=>loadInvoices('purchase'));
onElement('refreshSalesBtn','click',()=>loadInvoices('sale'));
onElement('cancelInvoiceEditor','click',()=>{document.getElementById('invoiceEditorOverlay').hidden=true;editingInvoiceId=null;});
onElement('purchaseInvoiceSearch','input',()=>{clearTimeout(window.__purchaseSearchTimer);window.__purchaseSearchTimer=setTimeout(()=>loadInvoices('purchase'),160);});
onElement('saleInvoiceSearch','input',()=>{clearTimeout(window.__saleSearchTimer);window.__saleSearchTimer=setTimeout(()=>loadInvoices('sale'),160);});
onElement('invoiceEditorForm','submit',async e=>{
 e.preventDefault();const btn=document.getElementById('saveInvoiceDraftBtn');const status=document.getElementById('invoiceEditorStatus');const type=currentInvoiceType;const rows=[...document.querySelectorAll('#invoiceLinesBody tr')];
 if(!rows.length){setInvoiceStatus(status,'أضف صنفًا واحدًا على الأقل للفاتورة.',false);return;}
 const items=[];for(const tr of rows){const productId=Number(tr.querySelector('.invoice-line-product').value);const quantityMilli=Math.round(Number(tr.querySelector('.invoice-line-qty').value)*1000);const unitPriceMinor=Math.round(Number(tr.querySelector('.invoice-line-price').value)*100);if(!productId||!Number.isFinite(quantityMilli)||quantityMilli<=0||!Number.isFinite(unitPriceMinor)||unitPriceMinor<0){setInvoiceStatus(status,'راجع الصنف والكمية والسعر في كل سطر.',false);return;}items.push({productId,quantityMilli,unitPriceMinor});}
 btn.disabled=true;try{let created;if(type==='quote'){created=await invoke('save_quotation',{quotationId:editingInvoiceId,customerId:Number(document.getElementById('invoiceParty').value),quoteDate:document.getElementById('invoiceDate').value,validUntil:document.getElementById('quoteValidUntil').value||null,discountMinor:Math.round(Number(document.getElementById('invoiceDiscount').value||0)*100),notes:document.getElementById('invoiceNotes').value.trim()||null,items});setInvoiceStatus(status,`${editingInvoiceId?'تم تحديث عرض السعر':'تم حفظ عرض السعر'} ${created.quoteNo} بإجمالي ${money(created.totalMinor)}.`,true);}else{const args={invoiceType:type,partyId:document.getElementById('invoiceParty').value?Number(document.getElementById('invoiceParty').value):null,invoiceDate:document.getElementById('invoiceDate').value,discountMinor:Math.round(Number(document.getElementById('invoiceDiscount').value||0)*100),notes:document.getElementById('invoiceNotes').value.trim()||null,items};if(editingInvoiceId)args.invoiceId=editingInvoiceId;created=await invoke(editingInvoiceId?'update_invoice_draft':'create_invoice',args);setInvoiceStatus(status,`${editingInvoiceId?'تم تحديث المسودة':'تم حفظ المسودة'} ${created.invoiceNo} بإجمالي ${money(created.totalMinor)}.`,true);}document.getElementById('invoiceEditorOverlay').hidden=true;editingInvoiceId=null;if(type==='quote')await loadQuotations();else await loadInvoices(type);await loadAuditLog('');}
 catch(err){setInvoiceStatus(status,'تعذر حفظ البيانات: '+err,false);}finally{btn.disabled=false;}
});
async function showInvoiceDetails(type,id){
 activePrintableDoc={type,id};
 const summary=document.getElementById('invoiceDetailsSummary');const body=document.getElementById('invoiceDetailsItems');summary.innerHTML='جارٍ تحميل التفاصيل...';body.innerHTML='';document.getElementById('invoiceDetailsOverlay').hidden=false;
 try{const d=await invoke('get_invoice_details',{invoiceType:type,invoiceId:id});const v=d.invoice;summary.innerHTML=`<div class="grid"><div><strong>رقم الفاتورة:</strong> <code>${escapeHtml(v.invoiceNo)}</code></div><div><strong>التاريخ:</strong> ${escapeHtml(v.invoiceDate)}</div><div><strong>${type==='purchase'?'المورد':'العميل'}:</strong> ${escapeHtml(v.partyName)}</div><div><strong>الحالة:</strong> ${invoiceStatusLabel(v.status)}</div><div><strong>الإجمالي قبل الخصم:</strong> ${money(v.subtotalMinor)}</div><div><strong>الخصم:</strong> ${money(v.discountMinor)}</div><div><strong>الضريبة (${(Number(v.taxRateBps||0)/100).toFixed(2)}%):</strong> ${money(v.taxMinor||0)}${v.pricesIncludeTax?' (متضمنة)':' (مضافة)'}</div><div><strong>الإجمالي:</strong> ${money(v.totalMinor)}</div><div><strong>أنشأها:</strong> ${escapeHtml(v.createdByName||'—')}</div></div><p class="muted">${escapeHtml(v.notes||'')}</p>`;body.innerHTML=d.items.map(i=>`<tr><td>${escapeHtml(i.productCode)}</td><td>${escapeHtml(i.productName)}</td><td>${escapeHtml(i.unit)}</td><td>${quantityBalanceText(i.quantityMilli)}</td><td>${money(i.unitPriceMinor)}</td><td>${money(i.lineTotalMinor)}</td></tr>`).join('')||'<tr><td colspan="6">لا توجد أصناف.</td></tr>';}
 catch(err){summary.textContent='تعذر تحميل تفاصيل الفاتورة: '+err;}
}
onElement('closeInvoiceDetails','click',()=>{document.getElementById('invoiceDetailsOverlay').hidden=true;});

// Company profile, quotations, receivables/payables, cashbook and reports.
let companyLogoDataUrl='';let currentPayableInvoices=[];

const localDateString=()=>{const d=new Date();return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;};
async function refreshDashboard(){
 if(!currentUser||!invoke)return;
 const fmt=n=>Number(n||0).toLocaleString(currentLanguage==='en'?'en-US':'ar-EG');
 const set=(id,value)=>{const el=document.getElementById(id);if(el)el.textContent=value;};
 try{const counts=await invoke('get_dashboard_inventory_counts');if(counts.productCount!=null)set('dashboardProductCount',fmt(counts.productCount));if(counts.inStockProductCount!=null)set('dashboardInStockProducts',fmt(counts.inStockProductCount));}catch(_){set('dashboardProductCount','—');set('dashboardInStockProducts','—');}
 if(currentUser.permissions.includes('reports.view')){
  const now=localDateString(),from=`${now.slice(0,7)}-01`;
  try{const r=await invoke('get_financial_summary',{fromDate:from,toDate:now});set('dashboardLowStockProducts',fmt(r.lowStockProducts));set('dashboardSalesMonthly',formatInvoiceMoney(r.salesMinor));set('dashboardPurchasesMonthly',formatInvoiceMoney(r.purchasesMinor));set('dashboardOutstandingSales',formatInvoiceMoney(r.outstandingSalesMinor));set('dashboardOutstandingPurchases',formatInvoiceMoney(r.outstandingPurchasesMinor));}catch(_){['dashboardLowStockProducts','dashboardSalesMonthly','dashboardPurchasesMonthly','dashboardOutstandingSales','dashboardOutstandingPurchases'].forEach(id=>set(id,'—'));}
 }
 if(currentUser.permissions.includes('quotations.view'))try{const quotes=await invoke('list_quotations',{query:null});set('dashboardOpenQuotations',fmt(quotes.filter(q=>['draft','sent','accepted'].includes(q.status)).length));}catch(_){set('dashboardOpenQuotations','—');}
 applyElementPermissions();
}

async function loadCompanyProfile(){if(!invoke||!currentUser)return;const form=document.getElementById('companyProfileForm');const canView=currentUser.permissions.some(p=>['company.settings.view','company.settings.manage'].includes(p));const canEdit=currentUser.permissions.includes('company.settings.manage');form.hidden=!canView;document.getElementById('companyReadonlyNotice').hidden=!canView||canEdit;form.querySelectorAll('input,select,textarea,button[type=submit]').forEach(el=>{el.disabled=!canEdit;});document.getElementById('companyLogoFile').disabled=!canEdit;document.getElementById('clearCompanyLogo').disabled=!canEdit;document.getElementById('saveCompanyProfileBtn').hidden=!canEdit;document.getElementById('clearCompanyLogo').hidden=!canEdit;try{const c=await invoke('get_company_profile');document.getElementById('companyName').value=c.name||'';document.getElementById('companyPhone').value=c.phone||'';document.getElementById('companyEmail').value=c.email||'';document.getElementById('companyAddress').value=c.address||'';document.getElementById('companyTaxNumber').value=c.tax_number||'';document.getElementById('companyCommercialRegister').value=c.commercial_register||'';document.getElementById('companyWebsite').value=c.website||'';document.getElementById('companyCurrency').value=c.currency_code||'EGP';document.getElementById('companyTaxRate').value=Number(c.tax_rate??0).toFixed(2).replace(/\.00$/,'');document.getElementById('companyPricesIncludeTax').value=String(Boolean(c.prices_include_tax));currentCurrency=c.currency_code||'EGP';document.getElementById('companyInvoiceFooter').value=c.invoice_footer||'';companyLogoDataUrl=c.logo_data_url||'';const img=document.getElementById('companyLogoPreview');img.src=companyLogoDataUrl;img.style.display=companyLogoDataUrl?'block':'none';}catch(err){const status=document.getElementById('companySettingsStatus');status.classList.remove('status-success');status.textContent=(currentLanguage==='en'?'Could not load company profile: ':'تعذر تحميل بيانات الشركة: ')+(currentLanguage==='en'?localizeText(String(err)):String(err));status.classList.add('status-error');}}
onElement('companyLogoFile','change',e=>{const f=e.target.files?.[0];if(!f)return;if(f.size>500000){document.getElementById('companySettingsStatus').textContent='حجم الشعار كبير. استخدم صورة أقل من 500 كيلوبايت.';e.target.value='';return;}const reader=new FileReader();reader.onload=()=>{companyLogoDataUrl=String(reader.result||'');const img=document.getElementById('companyLogoPreview');img.src=companyLogoDataUrl;img.style.display='block';};reader.readAsDataURL(f);});
onElement('clearCompanyLogo','click',()=>{companyLogoDataUrl='';document.getElementById('companyLogoFile').value='';const img=document.getElementById('companyLogoPreview');img.removeAttribute('src');img.style.display='none';});
onElement('companyProfileForm','submit',async e=>{e.preventDefault();const btn=document.getElementById('saveCompanyProfileBtn');const status=document.getElementById('companySettingsStatus');btn.disabled=true;status.classList.remove('status-error','status-success');try{await invoke('save_company_profile',{name:document.getElementById('companyName').value,phone:document.getElementById('companyPhone').value||null,email:document.getElementById('companyEmail').value||null,address:document.getElementById('companyAddress').value||null,taxNumber:document.getElementById('companyTaxNumber').value||null,commercialRegister:document.getElementById('companyCommercialRegister').value||null,website:document.getElementById('companyWebsite').value||null,logoDataUrl:companyLogoDataUrl||null,invoiceFooter:document.getElementById('companyInvoiceFooter').value||null,currencyCode:document.getElementById('companyCurrency').value,taxRate:Number(document.getElementById('companyTaxRate').value||0),pricesIncludeTax:document.getElementById('companyPricesIncludeTax').value==='true'});status.textContent=currentLanguage==='en'?'Company and tax settings saved successfully.':'تم حفظ بيانات الشركة وإعدادات الضريبة';status.classList.add('status-success');await loadHeaderBranding();}catch(err){status.textContent=(currentLanguage==='en'?'Could not save company settings: ':'تعذر حفظ بيانات الشركة: ')+(currentLanguage==='en'?localizeText(String(err)):String(err));status.classList.remove('status-success');status.classList.add('status-error');}finally{btn.disabled=false;}});
async function loadQuotations(query){if(!invoke||!currentUser)return;const body=document.getElementById('quotationsTable');const status=document.getElementById('quotationsStatus');try{const rows=await invoke('list_quotations',{query:query??(document.getElementById('quotationSearch').value||null)});body.innerHTML=rows.map(q=>{const b=[`<button class="btn secondary small-btn quote-details" data-id="${q.id}" type="button">تفاصيل</button>`,`<button class="btn secondary small-btn quote-print" data-id="${q.id}" type="button">طباعة</button>`];if(q.status==='draft')b.push(`<button class="btn secondary small-btn quote-edit" data-id="${q.id}" data-permission="quotations.edit" type="button">تعديل</button>`,`<button class="btn small-btn quote-status" data-id="${q.id}" data-next="sent" data-permission="quotations.status.manage" type="button">إرسال</button>`);if(['draft','sent'].includes(q.status))b.push(`<button class="btn secondary small-btn quote-status" data-id="${q.id}" data-next="accepted" data-permission="quotations.status.manage" type="button">قبول</button>`,`<button class="btn danger small-btn quote-status" data-id="${q.id}" data-next="rejected" data-permission="quotations.status.manage" type="button">رفض</button>`);if(q.status==='accepted')b.push(`<button class="btn small-btn quote-convert" data-id="${q.id}" data-permission="quotations.convert" type="button">تحويل لمبيعات</button>`);if(['draft','sent','accepted'].includes(q.status))b.push(`<button class="btn danger small-btn quote-status" data-id="${q.id}" data-next="cancelled" data-permission="quotations.status.manage" type="button">إلغاء</button>`);return `<tr><td><code>${escapeHtml(q.quoteNo)}</code></td><td>${escapeHtml(q.quoteDate)}</td><td>${escapeHtml(q.validUntil||'—')}</td><td>${escapeHtml(q.customerName)}</td><td>${q.itemCount}</td><td>${formatInvoiceMoney(q.totalMinor)}</td><td>${quotationStatusLabel(q.status)}</td><td><div class="table-actions">${b.join('')}</div></td></tr>`;}).join('')||'<tr><td colspan="8" class="muted">لا توجد عروض أسعار.</td></tr>';status.textContent=`عدد عروض الأسعار: ${rows.length}`;applyElementPermissions();body.querySelectorAll('.quote-print').forEach(b=>b.addEventListener('click',()=>printQuotationDocument(Number(b.dataset.id))));body.querySelectorAll('.quote-edit').forEach(b=>b.addEventListener('click',()=>openInvoiceEditor('quote',Number(b.dataset.id))));body.querySelectorAll('.quote-details').forEach(b=>b.addEventListener('click',()=>showQuotationDetails(Number(b.dataset.id))));body.querySelectorAll('.quote-status').forEach(b=>b.addEventListener('click',async()=>{if(!(await confirmProductAction(`تغيير حالة عرض السعر إلى ${quotationStatusLabel(b.dataset.next)}؟`,'تأكيد حالة العرض','تأكيد')))return;try{status.textContent=await invoke('set_quotation_status',{quotationId:Number(b.dataset.id),status:b.dataset.next});await loadQuotations();await loadAuditLog('');}catch(err){status.textContent='تعذر تحديث عرض السعر: '+err;}}));body.querySelectorAll('.quote-convert').forEach(b=>b.addEventListener('click',async()=>{if(!(await confirmProductAction('سيتم إنشاء مسودة فاتورة مبيعات من عرض السعر المقبول. لن يتم تعديل المخزون إلا عند اعتماد الفاتورة.','تحويل عرض السعر','إنشاء مسودة')))return;try{status.textContent=await invoke('convert_quotation_to_sale',{quotationId:Number(b.dataset.id)});await loadQuotations();await loadInvoices('sale');await loadAuditLog('');}catch(err){status.textContent='تعذر تحويل عرض السعر: '+err;}}));}catch(err){status.textContent='تعذر تحميل عروض الأسعار: '+err;}}
function quotationStatusLabel(status){const en=currentLanguage==='en';return ({draft:en?'Draft':'مسودة',sent:en?'Sent':'مرسل',accepted:en?'Accepted':'مقبول',rejected:en?'Rejected':'مرفوض',expired:en?'Expired':'منتهي',converted:en?'Converted':'محوّل',cancelled:en?'Cancelled':'ملغي'})[status]||status;}
async function showQuotationDetails(id){activePrintableDoc={type:'quote',id};const summary=document.getElementById('invoiceDetailsSummary');const body=document.getElementById('invoiceDetailsItems');summary.textContent='جارٍ تحميل التفاصيل...';body.innerHTML='';document.getElementById('invoiceDetailsOverlay').hidden=false;try{const d=await invoke('get_quotation_details',{quotationId:id});const q=d.quotation;summary.innerHTML=`<div class="grid"><div><strong>رقم العرض:</strong> <code>${escapeHtml(q.quoteNo)}</code></div><div><strong>التاريخ:</strong> ${escapeHtml(q.quoteDate)}</div><div><strong>صالح حتى:</strong> ${escapeHtml(q.validUntil||'—')}</div><div><strong>العميل:</strong> ${escapeHtml(q.customerName)}</div><div><strong>الحالة:</strong> ${quotationStatusLabel(q.status)}</div><div><strong>الضريبة (${(Number(q.taxRateBps||0)/100).toFixed(2)}%):</strong> ${formatInvoiceMoney(q.taxMinor||0)}${q.pricesIncludeTax?' (متضمنة)':' (مضافة)'}</div><div><strong>الإجمالي:</strong> ${formatInvoiceMoney(q.totalMinor)}</div></div><p>${escapeHtml(q.notes||'')}</p>`;body.innerHTML=d.items.map(i=>`<tr><td>${escapeHtml(i.productCode)}</td><td>${escapeHtml(i.productName)}</td><td>${escapeHtml(i.unit)}</td><td>${fmtQuantity(i.quantityMilli)}</td><td>${formatInvoiceMoney(i.unitPriceMinor)}</td><td>${formatInvoiceMoney(i.lineTotalMinor)}</td></tr>`).join('');}catch(err){summary.textContent='تعذر تحميل تفاصيل العرض: '+err;}}
async function refreshPayableInvoices(){if(!invoke)return;const type=document.getElementById('paymentInvoiceType').value;const select=document.getElementById('paymentInvoiceSelect');try{currentPayableInvoices=await invoke('list_payable_invoices',{invoiceType:type});select.innerHTML='<option value="">اختر الفاتورة</option>'+currentPayableInvoices.map(i=>`<option value="${i.invoiceId}">${escapeHtml(i.invoiceNo)} — ${escapeHtml(i.partyName)} — متبقي ${formatInvoiceMoney(i.balanceMinor)}</option>`).join('');document.getElementById('paymentInvoiceBalance').textContent='اختر فاتورة لعرض المتبقي.';}catch(err){select.innerHTML='<option value="">تعذر تحميل الفواتير</option>';document.getElementById('paymentStatus').textContent='تعذر تحميل الفواتير المفتوحة: '+err;}}
function updateSelectedPayable(){const row=currentPayableInvoices.find(x=>Number(x.invoiceId)===Number(document.getElementById('paymentInvoiceSelect').value));document.getElementById('paymentInvoiceBalance').textContent=row?`إجمالي الفاتورة ${formatInvoiceMoney(row.totalMinor)} — المدفوع ${formatInvoiceMoney(row.paidMinor)} — المتبقي ${formatInvoiceMoney(row.balanceMinor)}`:'اختر فاتورة لعرض المتبقي.';const amount=document.getElementById('paymentAmount');if(row)amount.value=(row.balanceMinor/100).toFixed(2);}
async function loadFinance(){if(!invoke||!currentUser)return;const typeSelect=document.getElementById('paymentInvoiceType');const purchaseOption=typeSelect.querySelector('option[value="purchase"]');if(purchaseOption){purchaseOption.hidden=!currentUser.permissions.includes('purchases.view');purchaseOption.disabled=!currentUser.permissions.includes('purchases.view');}if(typeSelect.value==='purchase'&&!currentUser.permissions.includes('purchases.view'))typeSelect.value='sale';const now=localDateString();document.getElementById('paymentDate').value=now;document.getElementById('cashAdjustmentDate').value=now;await refreshPayableInvoices();try{const rows=await invoke('list_payments');document.getElementById('paymentsTable').innerHTML=rows.map(p=>`<tr><td>${escapeHtml(p.paymentDate)}</td><td><code>${escapeHtml(p.invoiceNo||'—')}</code></td><td>${escapeHtml(p.partyName||'—')}</td><td>${p.direction==='in'?'تحصيل':'سداد'}</td><td>${formatInvoiceMoney(p.amountMinor)}</td><td>${paymentMethodLabel(p.method)}</td><td>${escapeHtml(p.createdByUsername||'—')}</td></tr>`).join('')||'<tr><td colspan="7" class="muted">لا توجد دفعات مسجلة.</td></tr>';}catch(err){document.getElementById('paymentStatus').textContent='تعذر تحميل الدفعات: '+err;}if(currentUser.permissions.includes('cashbox.view'))try{const r=await invoke('list_cashbook');document.getElementById('cashboxBalance').textContent=formatInvoiceMoney(r.balanceMinor);document.getElementById('cashbookTable').innerHTML=r.entries.map(x=>`<tr><td>${escapeHtml(x.entryDate)}</td><td>${escapeHtml(x.description)}</td><td>${x.direction==='in'?'وارد':'منصرف'}</td><td>${formatInvoiceMoney(x.amountMinor)}</td><td>${escapeHtml(x.createdByUsername||'—')}</td></tr>`).join('')||'<tr><td colspan="5" class="muted">لا توجد حركات خزنة.</td></tr>';}catch(err){document.getElementById('cashAdjustmentStatus').textContent='تعذر تحميل الخزنة: '+err;}}
function paymentMethodLabel(m){return ({cash:'نقدي',bank:'بنك',card:'بطاقة',transfer:'تحويل',other:'أخرى'})[m]||m;}
onElement('paymentInvoiceType','change',refreshPayableInvoices);onElement('paymentInvoiceSelect','change',updateSelectedPayable);onElement('refreshFinanceBtn','click',loadFinance);
onElement('paymentForm','submit',async e=>{e.preventDefault();const selected=document.getElementById('paymentInvoiceSelect').value;if(!selected){document.getElementById('paymentStatus').textContent='اختر فاتورة أولًا.';return;}const row=currentPayableInvoices.find(x=>Number(x.invoiceId)===Number(selected));if(!row)return;const btn=document.getElementById('savePaymentBtn');btn.disabled=true;try{document.getElementById('paymentStatus').textContent=await invoke('record_payment',{invoiceType:row.invoiceType,invoiceId:row.invoiceId,amountMinor:Math.round(Number(document.getElementById('paymentAmount').value)*100),paymentDate:document.getElementById('paymentDate').value,method:document.getElementById('paymentMethod').value,referenceNo:document.getElementById('paymentReference').value.trim()||null,notes:document.getElementById('paymentNotes').value.trim()||null});await loadFinance();await loadAuditLog('');}catch(err){document.getElementById('paymentStatus').textContent='تعذر تسجيل الدفعة: '+err;}finally{btn.disabled=false;}});
onElement('cashAdjustmentForm','submit',async e=>{e.preventDefault();const btn=e.submitter||e.currentTarget.querySelector('button[type=submit]');btn.disabled=true;try{document.getElementById('cashAdjustmentStatus').textContent=await invoke('cashbox_adjustment',{direction:document.getElementById('cashAdjustmentDirection').value,amountMinor:Math.round(Number(document.getElementById('cashAdjustmentAmount').value)*100),entryDate:document.getElementById('cashAdjustmentDate').value,description:document.getElementById('cashAdjustmentDescription').value,notes:document.getElementById('cashAdjustmentNotes').value.trim()||null});e.currentTarget.reset();document.getElementById('cashAdjustmentDate').value=localDateString();await loadFinance();await loadAuditLog('');}catch(err){document.getElementById('cashAdjustmentStatus').textContent='تعذر تسجيل التسوية: '+err;}finally{btn.disabled=false;}});
function renderReportCard(label,value){return `<article class="stat-card report-card"><div class="stat-label">${escapeHtml(label)}</div><div class="stat-value" style="font-size:21px">${escapeHtml(value)}</div></article>`;}
async function refreshReports(){if(!invoke||!currentUser)return;const now=localDateString();if(!document.getElementById('reportToDate').value)document.getElementById('reportToDate').value=now;if(!document.getElementById('reportFromDate').value)document.getElementById('reportFromDate').value=`${now.slice(0,7)}-01`;try{const r=await invoke('get_financial_summary',{fromDate:document.getElementById('reportFromDate').value,toDate:document.getElementById('reportToDate').value});document.getElementById('reportCards').innerHTML=[renderReportCard('إجمالي المبيعات المعتمدة',formatInvoiceMoney(r.salesMinor)),renderReportCard('إجمالي المشتريات المعتمدة',formatInvoiceMoney(r.purchasesMinor)),renderReportCard('ضريبة المبيعات',formatInvoiceMoney(r.salesTaxMinor||0)),renderReportCard('ضريبة المشتريات',formatInvoiceMoney(r.purchasesTaxMinor||0)),renderReportCard('تحصيلات العملاء',formatInvoiceMoney(r.receivedMinor)),renderReportCard('مدفوعات الموردين',formatInvoiceMoney(r.paidMinor)),renderReportCard('متبقي على العملاء',formatInvoiceMoney(r.outstandingSalesMinor)),renderReportCard('متبقي للموردين',formatInvoiceMoney(r.outstandingPurchasesMinor)),renderReportCard('الربح الإجمالي التقديري',formatInvoiceMoney(r.grossProfitMinor)),renderReportCard('أصناف منخفضة المخزون',String(r.lowStockProducts))].join('');document.getElementById('reportProfitCaveat').textContent=r.salesMissingCostSnapshot?`تنبيه: ${r.salesMissingCostSnapshot} فاتورة قديمة لا تحتوي تكلفة شراء محفوظة لكل صنف؛ الربح الإجمالي لا يشملها.`:'تم احتساب الربح الإجمالي من تكلفة الشراء المحفوظة وقت إنشاء سطر البيع بعد استبعاد ضريبة المبيعات. الضرائب المعروضة مستقلة.';document.getElementById('reportsStatus').textContent=`الفترة: ${r.fromDate} — ${r.toDate}`;}catch(err){document.getElementById('reportsStatus').textContent='تعذر إنشاء التقرير: '+err;}}
onElement('reportFilters','submit',e=>{e.preventDefault();refreshReports();});
onElement('newQuotationBtn','click',()=>openInvoiceEditor('quote'));onElement('refreshQuotationsBtn','click',()=>loadQuotations());onElement('quotationSearch','input',()=>{clearTimeout(window.__quotationSearchTimer);window.__quotationSearchTimer=setTimeout(()=>loadQuotations(),180);});
// Authentication, role-based permissions, account management and audit trail.
function applyElementPermissions(){
 if(!currentUser)return;
 document.querySelectorAll('[data-permission]').forEach(el=>{
  const permitted=currentUser.permissions.includes(el.dataset.permission);
  if(el.hasAttribute('data-page-content')){el.hidden=!permitted;return;}
  el.hidden=!permitted;
 });
 document.querySelectorAll('[data-any-permission]').forEach(el=>{
  const required=el.dataset.anyPermission.split(/\s+/).filter(Boolean);
  const permitted=required.some(code=>currentUser.permissions.includes(code));
  if(el.hasAttribute('data-page-content')){el.hidden=!permitted;return;}
  el.hidden=!permitted;
 });
 applyPriceFieldPermissions();
}
function applyPriceFieldPermissions(){
 if(!currentUser)return;
 const canPurchase=currentUser.permissions.includes('products.purchase_price.edit');
 const canSale=currentUser.permissions.includes('products.sale_price.edit');
 ['purchase','editPurchase'].forEach(id=>{const el=document.getElementById(id);if(el){el.disabled=!canPurchase;el.title=canPurchase?'':'لا تملك صلاحية تعديل سعر الشراء';}});
 ['sale','editSale'].forEach(id=>{const el=document.getElementById(id);if(el){el.disabled=!canSale;el.title=canSale?'':'لا تملك صلاحية تعديل سعر البيع';}});
 const canBarcode=currentUser.permissions.includes('products.barcode.manage');const barcodeField=document.getElementById('editBarcode');if(barcodeField){barcodeField.disabled=!canBarcode;barcodeField.title=canBarcode?'':'لا تملك صلاحية تغيير الباركود الأساسي';}
}
function showAuthPane(setupRequired){
 document.body.classList.add('auth-locked');
 document.getElementById('authOverlay').hidden=false;
 document.getElementById('authSetupPane').hidden=!setupRequired;
 document.getElementById('authLoginPane').hidden=setupRequired;
 document.getElementById('authStatus').textContent=setupRequired?'ابدأ بإنشاء حساب مدير النظام. يتطلب إنشاء الحساب كلمة مرور من 12 حرفًا على الأقل.':'';
 const focusId=setupRequired?'authSetupDisplayName':'authLoginUsername';document.getElementById(focusId).focus();
}
let userRowsCache=[];
let activeDetailUser=null;
let createAvatarDataUrl=null;
let detailAvatarDataUrl=null;
let myProfileAvatarDataUrl=null;
const roleLabels={admin:'مدير النظام',manager:'مدير',warehouse:'مسؤول مخزن',sales:'مبيعات',viewer:'عرض فقط'};
const roleLabelsEn={admin:'System administrator',manager:'Manager',warehouse:'Warehouse operator',sales:'Sales',viewer:'Read only'};
function localizedRoleLabel(code,fallback=''){
 if(currentLanguage==='en')return roleLabelsEn[code]||(fallback?localizeText(fallback):'');
 return roleLabels[code]||fallback||'';
}
const DEFAULT_AVATAR_SVG='<svg viewBox="0 0 64 64" aria-hidden="true" xmlns="http://www.w3.org/2000/svg"><circle cx="32" cy="22" r="12" fill="currentColor"/><path d="M8 58c1-13 10-21 24-21s23 8 24 21" fill="currentColor"/></svg>';
function avatarMarkup(user){
 const url=typeof user?.avatarDataUrl==='string'&&/^data:image\/(png|jpeg|webp);base64,/.test(user.avatarDataUrl)?user.avatarDataUrl:null;
 return `<span class="avatar-circle">${url?`<img alt="" src="${escapeHtml(url)}"/>`:DEFAULT_AVATAR_SVG}</span>`;
}
function setAvatarPreview(elementId,dataUrl,name){
 const node=document.getElementById(elementId);if(!node)return;
 const safeUrl=typeof dataUrl==='string'&&/^data:image\/(png|jpeg|webp);base64,/.test(dataUrl)?dataUrl:null;
 node.innerHTML=safeUrl?`<img alt="" src="${escapeHtml(safeUrl)}"/>`:DEFAULT_AVATAR_SVG;
}
function imageFileToDataUrl(file){
 return new Promise((resolve,reject)=>{
  if(!file)return resolve(null);
  if(!/^image\/(jpeg|png|webp)$/.test(file.type)){reject('استخدم صورة JPG أو PNG أو WebP فقط.');return;}
  if(file.size>5*1024*1024){reject('حجم الصورة يجب ألا يتجاوز 5 ميجابايت.');return;}
  const url=URL.createObjectURL(file);const img=new Image();
  img.onload=()=>{try{
   const side=256,scale=Math.max(side/img.width,side/img.height),w=Math.round(img.width*scale),h=Math.round(img.height*scale);
   const canvas=document.createElement('canvas');canvas.width=side;canvas.height=side;const ctx=canvas.getContext('2d');
   if(!ctx)throw new Error('تعذر تجهيز الصورة');
   ctx.drawImage(img,Math.round((side-w)/2),Math.round((side-h)/2),w,h);
   const data=canvas.toDataURL('image/jpeg',0.82);URL.revokeObjectURL(url);
   if(data.length>1500000)throw new Error('الصورة أكبر من الحد المسموح بعد الضغط.');resolve(data);
  }catch(e){URL.revokeObjectURL(url);reject(e.message||String(e));}};
  img.onerror=()=>{URL.revokeObjectURL(url);reject('تعذر قراءة الصورة المختارة.');};img.src=url;
 });
}
async function loadUsers(){
 const status=document.getElementById('usersListStatus');if(!currentUser?.permissions.includes('users.view'))return;
 try{
  const rows=await invoke('list_users');userRowsCache=rows;
  document.getElementById('usersTable').innerHTML=rows.map(u=>{
   const customization=u.permissionOverrides>0?`<small class="permission-source">${u.permissionOverrides} استثناءات فردية</small>`:'<small class="muted">حسب الدور</small>';
   const detailButton=`<button class="btn secondary small-btn view-user-details" data-id="${u.id}" type="button">عرض التفاصيل</button>`;
   const permissionButton=(currentUser.roleCode==='admin'&&currentUser.permissions.includes('users.permissions.manage')&&u.roleCode!=='admin'&&u.id!==currentUser.id)?`<button class="btn secondary small-btn edit-user-permissions" data-id="${u.id}" data-name="${escapeHtml(u.username)}" data-role="${escapeHtml(u.roleCode)}" type="button">الصلاحيات</button>`:'';
   return `<tr><td><div class="avatar-cell">${avatarMarkup(u)}<div><strong>${escapeHtml(u.displayName)}</strong><div class="muted"><code>${escapeHtml(u.username)}</code></div></div></div></td><td>${escapeHtml(localizedRoleLabel(u.roleCode,u.roleName))}<div>${customization}</div></td><td><span class="pill">${u.isActive?'نشط':'موقوف'}</span></td><td>${escapeHtml(u.lastLoginAt||'لم يسجل الدخول بعد')}</td><td><div class="table-actions">${detailButton}${permissionButton}</div></td></tr>`;
  }).join('')||'<tr><td colspan="5" class="muted">لا يوجد مستخدمون.</td></tr>';
  applyElementPermissions();
  document.querySelectorAll('.view-user-details').forEach(btn=>btn.addEventListener('click',()=>openUserDetail(Number(btn.dataset.id))));
  document.querySelectorAll('.edit-user-permissions').forEach(btn=>btn.addEventListener('click',()=>openUserPermissions(Number(btn.dataset.id),btn.dataset.name,btn.dataset.role)));
  const adminCreateOption=document.querySelector('#newUserRole option[value="admin"]');if(adminCreateOption)adminCreateOption.disabled=currentUser.roleCode!=='admin';
  if(status)status.textContent='';
 }catch(err){if(status)status.textContent='تعذر تحميل حسابات المستخدمين: '+err;}
}
function openCreateUserModal(){
 if(!currentUser?.permissions.includes('users.create'))return;
 document.getElementById('createUserForm').reset();createAvatarDataUrl=null;setAvatarPreview('newUserAvatarPreview',null,'م');
 document.getElementById('createUserStatus').textContent='';
 const adminOption=document.querySelector('#newUserRole option[value="admin"]');if(adminOption)adminOption.disabled=currentUser.roleCode!=='admin';
 document.getElementById('createUserOverlay').hidden=false;document.getElementById('newUserDisplayName').focus();
}
function openUserDetail(userId){
 const user=userRowsCache.find(item=>item.id===userId);if(!user)return;
 activeDetailUser=user;detailAvatarDataUrl=user.avatarDataUrl||null;
 document.getElementById('userDetailForm').reset();document.getElementById('detailUserId').value=String(user.id);
 document.getElementById('detailDisplayName').value=user.displayName||'';document.getElementById('detailUsername').value=user.username||'';
 document.getElementById('detailEmail').value=user.email||'';document.getElementById('detailPhone').value=user.phone||'';document.getElementById('detailAddress').value=user.address||'';
 document.getElementById('detailRole').value=user.roleCode;document.getElementById('detailNewPassword').value='';
 document.getElementById('detailDisplayHeading').textContent=user.displayName||user.username;
 document.getElementById('detailMetaHeading').textContent=`${user.username} · ${localizedRoleLabel(user.roleCode,user.roleName)}`;
 setAvatarPreview('detailUserAvatarPreview',detailAvatarDataUrl,user.displayName);
 const canProfile=currentUser.permissions.includes('users.profile.edit');
 ['detailDisplayName','detailEmail','detailPhone','detailAddress'].forEach(id=>document.getElementById(id).disabled=!canProfile);
 document.getElementById('detailUserAvatarFile').disabled=!canProfile;document.getElementById('clearDetailAvatar').hidden=!canProfile;
 document.getElementById('saveUserProfile').hidden=!canProfile;
 const self=user.id===currentUser.id;const isAdmin=user.roleCode==='admin';
 const canRole=currentUser.permissions.includes('users.role.manage')&&!self&&(!isAdmin||currentUser.roleCode==='admin');
 document.getElementById('detailRole').disabled=!canRole;
 document.getElementById('detailRole').querySelector('option[value="admin"]').disabled=currentUser.roleCode!=='admin';
 document.getElementById('saveDetailRole').hidden=!canRole;
 const canStatus=currentUser.permissions.includes('users.status.manage')&&!self&&(!isAdmin||currentUser.roleCode==='admin');
 document.getElementById('toggleDetailUser').hidden=!canStatus;document.getElementById('toggleDetailUser').disabled=false;
 document.getElementById('toggleDetailUser').textContent=user.isActive?'إيقاف الحساب':'إعادة تفعيل الحساب';
 document.getElementById('detailStatusLabel').textContent=user.isActive?'نشط':'موقوف';
 document.getElementById('openDetailPermissions').hidden=!(currentUser.roleCode==='admin'&&currentUser.permissions.includes('users.permissions.manage')&&!self&&!isAdmin);
 document.getElementById('detailPasswordSection').hidden=!(currentUser.permissions.includes('users.password.reset')&&!self&&(!isAdmin||currentUser.roleCode==='admin'));
 document.getElementById('detailUserStatus').textContent='';
 applyElementPermissions();document.getElementById('userDetailOverlay').hidden=false;
}
async function saveAvatarFromInput(inputId,mode){
 const input=document.getElementById(inputId);const file=input.files?.[0];if(!file)return;
 const cfg=mode==='create'?{status:'createUserStatus',submit:'createUserBtn',preview:'newUserAvatarPreview',name:'newUserDisplayName'}:mode==='detail'?{status:'detailUserStatus',submit:'saveUserProfile',preview:'detailUserAvatarPreview',name:'detailDisplayName'}:{status:'myProfileStatus',submit:'saveMyProfileBtn',preview:'myProfileAvatarPreview',name:'myProfileDisplayName'};
 const status=document.getElementById(cfg.status);const submit=document.getElementById(cfg.submit);if(status)status.textContent='جارٍ تجهيز الصورة...';if(submit)submit.disabled=true;
 try{const data=await imageFileToDataUrl(file);if(mode==='create')createAvatarDataUrl=data;else if(mode==='detail')detailAvatarDataUrl=data;else myProfileAvatarDataUrl=data;setAvatarPreview(cfg.preview,data,document.getElementById(cfg.name).value);if(status)status.textContent='تم تجهيز الصورة.';}
 catch(err){if(status)status.textContent='تعذر تجهيز الصورة: '+err;input.value='';}
 finally{if(submit)submit.disabled=false;}
}
const permissionGroupInfo={
 'products.':{title:'المنتجات',order:1},
 'stock.':{title:'المخزون',order:2},
 'backup.':{title:'النسخ الاحتياطي والاستعادة',order:3},
 'users.':{title:'المستخدمون والحسابات',order:4},
 'audit.':{title:'سجل النشاط',order:5}
};
let permissionEditorUserId=null;
let permissionEditorRows=[];
function permissionGroupFor(code){return Object.keys(permissionGroupInfo).find(prefix=>code.startsWith(prefix))||'other.';}
async function openUserPermissions(userId,username,roleCode){
 const overlay=document.getElementById('userPermissionsOverlay');
 const status=document.getElementById('permissionEditorStatus');
 const fields=document.getElementById('permissionEditorFields');
 permissionEditorUserId=userId;permissionEditorRows=[];
 document.getElementById('userPermissionsTitle').textContent=`تخصيص الصلاحيات — ${username}`;
 status.textContent='جارٍ تحميل الصلاحيات...';fields.innerHTML='';overlay.hidden=false;
 try{
  const rows=await invoke('list_user_permissions',{userId});permissionEditorRows=rows;
  const groups={};
  for(const row of rows){const key=permissionGroupFor(row.code);(groups[key]??=[]).push(row);}
  const ordered=Object.entries(groups).sort((a,b)=>(permissionGroupInfo[a[0]]?.order||99)-(permissionGroupInfo[b[0]]?.order||99));
  fields.innerHTML=ordered.map(([key,items])=>`<section class="permission-group"><h4>${escapeHtml(permissionGroupInfo[key]?.title||'صلاحيات أخرى')}</h4>${items.map(item=>{
   const locked=item.code==='users.permissions.manage';
   const source=item.isOverride?'تخصيص فردي':item.roleAllowed?'افتراضي الدور':'غير ممنوحة بالدور';
   return `<label class="permission-option"><input class="permission-check" type="checkbox" data-code="${escapeHtml(item.code)}" ${item.allowed?'checked':''} ${locked?'disabled':''}/><span><strong>${escapeHtml(item.name)}</strong><small>${escapeHtml(item.description)}</small><small class="permission-source">${escapeHtml(locked?'مدير النظام فقط':source)}</small></span></label>`;
  }).join('')}</section>`).join('');
  // Keep the elevated permission locked and unchecked for non-admin user accounts.
  const special=fields.querySelector('[data-code="users.permissions.manage"]');if(special){special.checked=false;special.disabled=true;}
  document.getElementById('selectAllPermissions').onclick=()=>fields.querySelectorAll('.permission-check:not(:disabled)').forEach(el=>el.checked=true);
  document.getElementById('clearPermissions').onclick=()=>fields.querySelectorAll('.permission-check:not(:disabled)').forEach(el=>el.checked=false);
  document.getElementById('selectRolePermissions').onclick=()=>fields.querySelectorAll('.permission-check').forEach(el=>{const row=permissionEditorRows.find(item=>item.code===el.dataset.code);if(row&&!el.disabled)el.checked=row.roleAllowed;});
  status.textContent='عدّل الاختيارات ثم احفظ. المنح الفردية تتجاوز افتراضات الدور، ويمكن إرجاع الاختيارات إلى إعدادات الدور.';
 }catch(err){status.textContent='تعذر تحميل صلاحيات المستخدم: '+err;}
}
onElement('cancelUserPermissions','click',()=>{document.getElementById('userPermissionsOverlay').hidden=true;permissionEditorUserId=null;});
onElement('saveUserPermissions','click',async()=>{
 const button=document.getElementById('saveUserPermissions');const status=document.getElementById('permissionEditorStatus');
 if(!permissionEditorUserId)return;
 const allowedCodes=[...document.querySelectorAll('#permissionEditorFields .permission-check:checked')].map(el=>el.dataset.code);
 button.disabled=true;status.textContent='جارٍ حفظ الصلاحيات...';
 try{status.textContent=await invoke('set_user_permissions',{userId:permissionEditorUserId,allowedCodes});await loadUsers();await loadAuditLog('');setTimeout(()=>{document.getElementById('userPermissionsOverlay').hidden=true;permissionEditorUserId=null;},250);}
 catch(err){status.textContent='تعذر حفظ الصلاحيات: '+err;}
 finally{button.disabled=false;}
});
function auditEntityLabel(type){return ({product:'صنف',stock:'مخزون',user:'مستخدم',session:'جلسة',backup:'نسخة احتياطية',database:'قاعدة البيانات',customers:'عميل',suppliers:'مورد',purchase_invoice:'فاتورة مشتريات',sale_invoice:'فاتورة مبيعات',quotation:'عرض سعر',payment:'دفعة',cashbox:'الخزنة',company:'بيانات الشركة'})[type]||type;}
function auditActionLabel(action){return ({initial_admin_created:'إنشاء مدير النظام',login:'تسجيل دخول ناجح',login_failed:'محاولة دخول فاشلة',logout:'تسجيل خروج',create:'إنشاء صنف',update:'تعديل صنف',deactivate:'إيقاف صنف',reactivate:'إعادة تفعيل صنف',add_alternate_barcode:'إضافة باركود بديل',stock_in:'وارد مخزون',stock_out:'منصرف مخزون',create_user:'إنشاء مستخدم',user_created:'إنشاء مستخدم',user_deactivated:'إيقاف حساب مستخدم',user_activated:'تفعيل حساب مستخدم',user_role_changed:'تغيير دور مستخدم',user_permissions_changed:'تخصيص صلاحيات مستخدم',user_profile_updated:'تعديل بيانات مستخدم',password_reset:'إعادة تعيين كلمة مرور',password_changed:'تغيير كلمة مرور',backup_created:'إنشاء نسخة احتياطية',restore_point_deleted:'حذف نقطة استعادة',backup_restored:'استعادة قاعدة البيانات',customer_created:'إضافة عميل',customer_updated:'تعديل عميل',customer_deactivated:'إيقاف عميل',customer_activated:'تفعيل عميل',supplier_created:'إضافة مورد',supplier_updated:'تعديل مورد',supplier_deactivated:'إيقاف مورد',supplier_activated:'تفعيل مورد',purchase_invoice_created:'إنشاء مسودة مشتريات',purchase_invoice_updated:'تعديل مسودة مشتريات',purchase_invoice_posted:'اعتماد مشتريات',purchase_invoice_cancelled:'إلغاء مشتريات',sale_invoice_created:'إنشاء مسودة مبيعات',sale_invoice_updated:'تعديل مسودة مبيعات',sale_invoice_posted:'اعتماد مبيعات',sale_invoice_cancelled:'إلغاء مبيعات',quotation_created:'إنشاء عرض سعر',quotation_updated:'تعديل عرض سعر',quotation_status_changed:'تغيير حالة عرض سعر',quotation_converted:'تحويل عرض سعر إلى فاتورة',payment_recorded:'تسجيل دفعة',cashbox_adjustment:'تسوية الخزنة',company_profile_updated:'تعديل بيانات الشركة'})[action]||action;}
function localAuditDate(value){if(!value)return '';const parsed=new Date(value.replace(' ','T')+'Z');return Number.isNaN(parsed.getTime())?value:parsed.toLocaleString(currentLanguage==='en'?'en-GB':'ar-EG');}
async function loadAuditLog(query){
 const status=document.getElementById('auditStatus');if(!currentUser?.permissions.includes('audit.view'))return;
 const search=document.getElementById('auditSearch');
 if(typeof query==='string')search.value=query;
 const term=search.value.trim();
 const entityType=document.getElementById('auditTypeFilter').value.trim();
 const action=document.getElementById('auditActionFilter').value.trim();
 try{
  const rows=await invoke('list_audit_log',{query:term,entityType,action});
  document.getElementById('auditTable').innerHTML=rows.map(row=>`<tr><td>${escapeHtml(localAuditDate(row.createdAt))}</td><td>${escapeHtml(row.actorUsername||'النظام/غير معروف')}</td><td>${escapeHtml(auditActionLabel(row.action))}</td><td>${escapeHtml(auditEntityLabel(row.entityType))}</td><td>${escapeHtml(row.entityId||'—')}</td><td>${escapeHtml(row.details||'')}</td></tr>`).join('')||'<tr><td colspan="6" class="muted">لا توجد عمليات مسجلة مطابقة للبحث.</td></tr>';
  const activeFilters=[term,entityType,action].filter(Boolean).length;
  status.textContent=`عدد السجلات المعروضة: ${rows.length}${activeFilters?` · المرشحات النشطة: ${activeFilters}`:''}`;
 }catch(err){status.textContent='تعذر تحميل سجل النشاط: '+err;}
}
async function finishLogin(user){
  currentUser=user;document.body.classList.remove('auth-locked');document.getElementById('authOverlay').hidden=true;renderHeaderUser(user);document.getElementById('logoutBtn').hidden=false;document.getElementById('changePasswordBtn').hidden=false;closeHeaderMenus();await loadHeaderBranding();
  applyElementPermissions();await showPage('dashboard');
}
async function initializeAuth(){
 if(!window.MadarAPI.isDesktop){document.getElementById('authStatus').textContent='وضع المتصفح للتجربة فقط: البيانات تُحفظ في هذا المتصفح ولا تتصل بقاعدة بيانات سطح المكتب.';}
 try{const result=await invoke('auth_status');if(result.setupRequired){showAuthPane(true);return;}if(result.currentUser){await finishLogin(result.currentUser);return;}showAuthPane(false);}
 catch(err){showAuthPane(false);document.getElementById('authStatus').textContent='تعذر فحص إعداد المستخدمين: '+err;}
}
function openMyProfile(){
 if(!currentUser)return;
 closeHeaderMenus();
 myProfileAvatarDataUrl=currentUser.avatarDataUrl||null;
 document.getElementById('myProfileForm').reset();
 document.getElementById('myProfileDisplayName').value=currentUser.displayName||'';
 document.getElementById('myProfileUsername').value=currentUser.username||'';
 document.getElementById('myProfileEmail').value=currentUser.email||'';
 document.getElementById('myProfilePhone').value=currentUser.phone||'';
 document.getElementById('myProfileAddress').value=currentUser.address||'';
 document.getElementById('myProfileAvatarFile').value='';
 setAvatarPreview('myProfileAvatarPreview',myProfileAvatarDataUrl,currentUser.displayName);
 const status=document.getElementById('myProfileStatus');status.textContent='';status.className='status';
 document.getElementById('myProfileOverlay').hidden=false;document.getElementById('myProfileDisplayName').focus();
}
onElement('editMyProfileBtn','click',openMyProfile);
onElement('cancelMyProfileBtn','click',()=>{document.getElementById('myProfileOverlay').hidden=true;});
onElement('myProfileAvatarFile','change',()=>saveAvatarFromInput('myProfileAvatarFile','self'));
onElement('clearMyProfileAvatar','click',()=>{myProfileAvatarDataUrl=null;document.getElementById('myProfileAvatarFile').value='';setAvatarPreview('myProfileAvatarPreview',null,document.getElementById('myProfileDisplayName').value);});
onElement('myProfileDisplayName','input',()=>{if(!myProfileAvatarDataUrl)setAvatarPreview('myProfileAvatarPreview',null,document.getElementById('myProfileDisplayName').value);});
onElement('myProfileForm','submit',async e=>{
 e.preventDefault();if(!currentUser)return;
 const button=document.getElementById('saveMyProfileBtn');const status=document.getElementById('myProfileStatus');button.disabled=true;status.className='status';status.textContent=currentLanguage==='en'?'Saving your profile…':'جارٍ حفظ بياناتك الشخصية…';
 const profile={displayName:document.getElementById('myProfileDisplayName').value.trim(),email:document.getElementById('myProfileEmail').value.trim()||null,phone:document.getElementById('myProfilePhone').value.trim()||null,address:document.getElementById('myProfileAddress').value.trim()||null,avatarDataUrl:myProfileAvatarDataUrl};
 try{
  const result=await invoke('update_user_profile',{userId:currentUser.id,...profile});
  Object.assign(currentUser,profile);renderHeaderUser(currentUser);
  status.className='status status-success';status.textContent=currentLanguage==='en'?'Your profile was saved successfully.':result;
  const userName=document.getElementById('userMenuName');if(userName)userName.textContent=currentUser.displayName;
 }catch(err){status.className='status status-error';status.textContent=(currentLanguage==='en'?'Could not save your profile: ':'تعذر حفظ بياناتك الشخصية: ')+ (currentLanguage==='en'?localizeText(String(err)):String(err));console.error('[Madar] self profile update failed:',err);}
 finally{button.disabled=false;}
});

onElement('authSetupForm','submit',async e=>{
 e.preventDefault();const btn=document.getElementById('authSetupSubmit');const status=document.getElementById('authStatus');const password=document.getElementById('authSetupPassword').value;
 btn.disabled=true;status.textContent='جارٍ إنشاء حساب مدير النظام...';
 try{const user=await invoke('create_initial_admin',{username:document.getElementById('authSetupUsername').value,displayName:document.getElementById('authSetupDisplayName').value,password});document.getElementById('authSetupForm').reset();await finishLogin(user);}
 catch(err){status.textContent='تعذر إنشاء حساب المدير: '+err;}
 finally{btn.disabled=false;}
});
onElement('authLoginForm','submit',async e=>{
 e.preventDefault();const btn=document.getElementById('authLoginSubmit');const status=document.getElementById('authStatus');btn.disabled=true;status.textContent='جارٍ التحقق من بيانات الدخول...';
 try{const user=await invoke('login',{username:document.getElementById('authLoginUsername').value,password:document.getElementById('authLoginPassword').value});document.getElementById('authLoginPassword').value='';await finishLogin(user);}
 catch(err){status.textContent='فشل تسجيل الدخول: '+err;document.getElementById('authLoginPassword').value='';document.getElementById('authLoginPassword').focus();}
 finally{btn.disabled=false;}
});
onElement('logoutBtn','click',async()=>{document.body.classList.add('auth-locked');document.getElementById('authOverlay').hidden=false;document.getElementById('authSetupPane').hidden=true;document.getElementById('authLoginPane').hidden=true;document.getElementById('authStatus').textContent='جارٍ تسجيل الخروج...';try{await invoke('logout');}catch(err){console.error(err);}currentUser=null;document.getElementById('currentUserBadge').hidden=true;document.getElementById('logoutBtn').hidden=true;document.getElementById('changePasswordBtn').hidden=true;closeHeaderMenus();renderHeaderUser(null);renderHeaderCompany(null);showAuthPane(false);});
onElement('openCreateUser','click',openCreateUserModal);
onElement('cancelCreateUser','click',()=>{document.getElementById('createUserOverlay').hidden=true;});
onElement('newUserAvatarFile','change',()=>saveAvatarFromInput('newUserAvatarFile','create'));
onElement('clearNewUserAvatar','click',()=>{createAvatarDataUrl=null;document.getElementById('newUserAvatarFile').value='';setAvatarPreview('newUserAvatarPreview',null,document.getElementById('newUserDisplayName').value);});
onElement('newUserDisplayName','input',()=>{if(!createAvatarDataUrl)setAvatarPreview('newUserAvatarPreview',null,document.getElementById('newUserDisplayName').value);});
onElement('createUserForm','submit',async e=>{
 e.preventDefault();const status=document.getElementById('createUserStatus');const btn=document.getElementById('createUserBtn');btn.disabled=true;
 try{
  const result=await invoke('create_user',{username:document.getElementById('newUserUsername').value,displayName:document.getElementById('newUserDisplayName').value,roleCode:document.getElementById('newUserRole').value,password:document.getElementById('newUserPassword').value,email:document.getElementById('newUserEmail').value.trim()||null,phone:document.getElementById('newUserPhone').value.trim()||null,address:document.getElementById('newUserAddress').value.trim()||null,avatarDataUrl:createAvatarDataUrl});
  status.textContent=result;document.getElementById('createUserForm').reset();createAvatarDataUrl=null;setAvatarPreview('newUserAvatarPreview',null,'م');document.getElementById('createUserOverlay').hidden=true;
  await loadUsers();const listStatus=document.getElementById('usersListStatus');if(listStatus)listStatus.textContent=result;await loadAuditLog('');
 }catch(err){status.textContent='تعذر إنشاء المستخدم: '+err;}
 finally{btn.disabled=false;}
});
onElement('refreshUsersBtn','click',loadUsers);
onElement('closeUserDetail','click',()=>{document.getElementById('userDetailOverlay').hidden=true;activeDetailUser=null;});
onElement('detailUserAvatarFile','change',()=>saveAvatarFromInput('detailUserAvatarFile','detail'));
onElement('clearDetailAvatar','click',()=>{detailAvatarDataUrl=null;document.getElementById('detailUserAvatarFile').value='';setAvatarPreview('detailUserAvatarPreview',null,document.getElementById('detailDisplayName').value);});
onElement('detailDisplayName','input',()=>{if(!detailAvatarDataUrl)setAvatarPreview('detailUserAvatarPreview',null,document.getElementById('detailDisplayName').value);});
onElement('userDetailForm','submit',async e=>{
 e.preventDefault();if(!activeDetailUser)return;const status=document.getElementById('detailUserStatus');const button=document.getElementById('saveUserProfile');button.disabled=true;
 try{const result=await invoke('update_user_profile',{userId:activeDetailUser.id,displayName:document.getElementById('detailDisplayName').value,email:document.getElementById('detailEmail').value.trim()||null,phone:document.getElementById('detailPhone').value.trim()||null,address:document.getElementById('detailAddress').value.trim()||null,avatarDataUrl:detailAvatarDataUrl});
  await loadUsers();activeDetailUser=userRowsCache.find(u=>u.id===activeDetailUser.id)||activeDetailUser;
  document.getElementById('detailDisplayHeading').textContent=activeDetailUser.displayName;document.getElementById('detailMetaHeading').textContent=`${activeDetailUser.username} · ${localizedRoleLabel(activeDetailUser.roleCode,activeDetailUser.roleName)}`;
  if(currentUser.id===activeDetailUser.id){currentUser.displayName=activeDetailUser.displayName;currentUser.avatarDataUrl=detailAvatarDataUrl||null;renderHeaderUser(currentUser);}
  status.textContent=result;await loadAuditLog('');
 }catch(err){status.textContent='تعذر حفظ بيانات المستخدم: '+err;}
 finally{button.disabled=false;}
});
onElement('saveDetailRole','click',async()=>{
 if(!activeDetailUser)return;const status=document.getElementById('detailUserStatus');const button=document.getElementById('saveDetailRole');button.disabled=true;
 try{const result=await invoke('set_user_role',{userId:activeDetailUser.id,roleCode:document.getElementById('detailRole').value});await loadUsers();activeDetailUser=userRowsCache.find(u=>u.id===activeDetailUser.id)||activeDetailUser;openUserDetail(activeDetailUser.id);document.getElementById('detailUserStatus').textContent=result;await loadAuditLog('');}
 catch(err){status.textContent='تعذر تغيير دور المستخدم: '+err;}finally{button.disabled=false;}
});
onElement('toggleDetailUser','click',async()=>{
 if(!activeDetailUser)return;const active=!activeDetailUser.isActive;const userId=activeDetailUser.id;const msg=active?`هل تريد إعادة تفعيل الحساب ${activeDetailUser.username}؟`:`هل تريد إيقاف الحساب ${activeDetailUser.username}؟ لن يستطيع تسجيل الدخول بعد ذلك.`;
 if(!(await confirmProductAction(msg,'إدارة حساب مستخدم',active?'نعم، فعّل الحساب':'نعم، أوقف الحساب')))return;
 const status=document.getElementById('detailUserStatus');const button=document.getElementById('toggleDetailUser');button.disabled=true;
 try{const result=await invoke('set_user_active',{userId,active});await loadUsers();activeDetailUser=userRowsCache.find(u=>u.id===userId)||activeDetailUser;openUserDetail(userId);document.getElementById('detailUserStatus').textContent=result;await loadAuditLog('');}
 catch(err){status.textContent='تعذر تحديث حالة الحساب: '+err;}finally{button.disabled=false;}
});
onElement('openDetailPermissions','click',()=>{if(activeDetailUser)openUserPermissions(activeDetailUser.id,activeDetailUser.username,activeDetailUser.roleCode);});
onElement('detailResetPasswordForm','submit',async e=>{
 e.preventDefault();if(!activeDetailUser)return;const status=document.getElementById('detailUserStatus');const button=document.getElementById('detailResetPasswordSubmit');button.disabled=true;
 try{status.textContent=await invoke('reset_user_password',{userId:activeDetailUser.id,newPassword:document.getElementById('detailNewPassword').value});document.getElementById('detailNewPassword').value='';await loadAuditLog('');}
 catch(err){status.textContent='تعذر تغيير كلمة المرور: '+err;}finally{button.disabled=false;}
});
onElement('changePasswordBtn','click',()=>{closeHeaderMenus();document.getElementById('changeOwnPasswordForm').reset();document.getElementById('ownPasswordStatus').textContent='';document.getElementById('changePasswordOverlay').hidden=false;document.getElementById('ownCurrentPassword').focus();});
onElement('cancelChangePassword','click',()=>document.getElementById('changePasswordOverlay').hidden=true);
onElement('changeOwnPasswordForm','submit',async e=>{e.preventDefault();const button=document.getElementById('changeOwnPasswordSubmit');const status=document.getElementById('ownPasswordStatus');button.disabled=true;try{status.textContent=await invoke('change_own_password',{currentPassword:document.getElementById('ownCurrentPassword').value,newPassword:document.getElementById('ownNewPassword').value});document.getElementById('ownCurrentPassword').value='';document.getElementById('ownNewPassword').value='';}catch(err){status.textContent='تعذر تغيير كلمة المرور: '+err;}finally{button.disabled=false;}});
onElement('refreshAuditBtn','click',()=>loadAuditLog());
onElement('resetAuditFilters','click',()=>{document.getElementById('auditSearch').value='';document.getElementById('auditTypeFilter').value='';document.getElementById('auditActionFilter').value='';loadAuditLog();});
let auditSearchTimer;onElement('auditSearch','input',e=>{clearTimeout(auditSearchTimer);auditSearchTimer=setTimeout(()=>loadAuditLog(e.target.value),180);});
onElement('auditTypeFilter','change',()=>loadAuditLog());
onElement('auditActionFilter','change',()=>loadAuditLog());

// Lightweight bilingual UI layer. Original Arabic text is retained on each node so switching
// languages back and forth never translates an already translated string.
const arEn={
'مدار':'Madar','نظام إدارة المبيعات والمخازن والحسابات':'Sales, Inventory and Accounts Management','نسخة تطوير 0.3.0':'Development build 0.3.0','تسجيل الخروج':'Log out','تغيير كلمة المرور':'Change password','English':'العربية','القائمة الرئيسية':'Main menu','الرئيسية':'Dashboard','المنتجات':'Products','المخزون':'Inventory','النسخ الاحتياطي':'Backups','المستخدمون':'Users','سجل النشاط':'Activity log','المساعدة':'Help','لوحة التحكم':'Dashboard','نظرة سريعة على مدار، مع وصول مباشر للأقسام الأساسية.':'A quick overview of Madar and shortcuts to the main sections.','الأصناف المسجلة':'Registered products','حالة قاعدة البيانات':'Database status','محلية':'Local','نظام الباركود':'Barcode system','مفعّل':'Enabled','أصناف لها رصيد بالمخزون':'Products in stock','اختصارات سريعة':'Quick shortcuts','اختار القسم اللي عايز تشتغل فيه.':'Choose the section you want to work in.','إدارة المنتجات':'Manage products','إدارة المخزون':'Manage inventory','النسخ الاحتياطي والاستعادة':'Backup and restore','المساعدة وقواعد الباركود':'Help and barcode rules','حالة النظام':'System status','يعمل مدار بقاعدة بيانات SQLite محلية. احرص على إنشاء نسخة احتياطية دورية قبل أي تغييرات كبيرة على البيانات.':'Madar uses a local SQLite database. Create backups regularly before major data changes.','المستخدمون والصلاحيات':'Users and permissions','افتح تفاصيل المستخدم لعرض بياناته وتعديلها وإدارة كلمة المرور أو حالة الحساب والصلاحيات.':'Open a user profile to view or edit their details, password, account status, and permissions.','حسابات المستخدمين':'User accounts','＋ إضافة مستخدم':'+ Add user','إضافة مستخدم':'Add user','تحديث القائمة':'Refresh list','المستخدم':'User','اسم العرض':'Display name','الدور':'Role','الحالة':'Status','آخر دخول':'Last login','إجراءات':'Actions','جارٍ تحميل المستخدمين...':'Loading users...','لا يوجد مستخدمون.':'No users found.','مدير النظام':'System administrator','مدير':'Manager','مسؤول مخزن':'Warehouse operator','مبيعات':'Sales','عرض فقط':'Read only','نشط':'Active','موقوف':'Inactive','لم يسجل الدخول بعد':'Never signed in','عرض التفاصيل':'View details','الصلاحيات':'Permissions','حسب الدور':'Role defaults','استثناءات فردية':'custom overrides','تفاصيل المستخدم':'User details','البيانات الشخصية':'Personal details','الصورة الشخصية':'Profile photo','تغيير الصورة':'Change photo','استخدام الصورة الافتراضية':'Use default avatar','الاسم الظاهر':'Display name','اسم المستخدم':'Username','البريد الإلكتروني':'Email address','رقم التليفون':'Phone number','العنوان':'Address','حفظ البيانات الشخصية':'Save profile','الحساب والصلاحيات':'Account and permissions','إيقاف الحساب':'Deactivate account','إعادة تفعيل الحساب':'Reactivate account','إعادة تعيين كلمة المرور':'Reset password','كلمة المرور الجديدة (12 حرفًا على الأقل)':'New password (at least 12 characters)','حفظ الدور':'Save role','تخصيص الصلاحيات':'Customize permissions','إغلاق':'Close','إلغاء':'Cancel','إنشاء المستخدم':'Create user','كلمة المرور المؤقتة (12 حرفًا على الأقل)':'Temporary password (at least 12 characters)','املأ البيانات الأساسية؛ البريد والهاتف والعنوان والصورة بيانات اختيارية.':'Enter the basic details. Email, phone, address, and photo are optional.','كلمة المرور المؤقتة (12 حرفًا على الأقل)':'Temporary password (at least 12 characters)','مثال: cashier01':'e.g. cashier01','اسم الموظف':'Employee name','رقم التواصل':'Contact number','العنوان (اختياري)':'Address (optional)','تأكيد كلمة المرور':'Confirm password','بحث في السجل':'Search activity log','اسم المستخدم أو العملية أو التفاصيل':'Username, action, or details','تصفية حسب النوع':'Filter by type','كل الأنواع':'All types','تصفية حسب العملية':'Filter by action','كل العمليات':'All actions','تحديث السجل':'Refresh log','التاريخ والوقت':'Date and time','العملية':'Action','النوع':'Type','المعرّف':'ID','التفاصيل':'Details','لا توجد عمليات مسجلة مطابقة للبحث.':'No matching activity found.','المساعدة':'Help','معلومات أساسية عن مدار وقواعد استخدام الباركود.':'Basic information about Madar and barcode rules.','أهلًا بك في مدار':'Welcome to Madar','الباركود فريد ولا يتكرر':'Barcodes are unique','دعم قارئ الباركود USB كإدخال لوحة مفاتيح':'USB barcode scanner supported as keyboard input','قواعد الباركود':'Barcode rules','إضافة صنف جديد':'Add new product','كود الصنف':'Product code','اسم الصنف':'Product name','الوحدة':'Unit','الباركود الأساسي (اختياري)':'Primary barcode (optional)','سعر الشراء':'Purchase price','سعر البيع':'Sale price','حفظ الصنف':'Save product','حفظ الباركود':'Save barcode','الباركود البديل':'Alternate barcode','الباركود يُعامل كنص للحفاظ على الأصفار في بدايته. من نافذة «تعديل» يمكنك تغيير الباركود الأساسي صراحةً؛ أما «باركود بديل» فيضيف رمزًا إضافيًا دون تغيير الأساسي.':'Barcodes are treated as text to preserve leading zeros. In the Edit dialog, you can explicitly change the primary barcode; Alternate barcode adds another code without changing the primary.','تم تعديل «':'Updated “','». الباركود الأساسي الحالي: ':'”. Current primary barcode: ','تم الحفاظ على الباركود السابق إذا تم تغييره.':'The previous barcode was retained if it was changed.','فشل حفظ التعديلات: ':'Failed to save changes: ','الباركود الأساسي':'Primary barcode','عدّل بيانات الصنف، وسيتم الاحتفاظ بالباركودات السابقة وعدم حذفها.':'Edit the product details. Existing barcodes will be retained, not deleted.','يمكنك تغيير الباركود الأساسي صراحةً؛ عند تغييره نحتفظ بالباركود السابق كبديل.':'You can explicitly change the primary barcode. The previous barcode will be retained as an alternate.','سيظهر هذا الباركود في قائمة المنتجات؛ لا يمكن استخدام باركود لصنف آخر.':'This barcode appears in the product list. A barcode assigned to another product cannot be used.','الباركود الأساسي لا يمكن أن يكون فارغًا':'The primary barcode cannot be empty','لا تملك صلاحية تغيير الباركود الأساسي':'You do not have permission to change the primary barcode','الباركود الأساسي':'Primary barcode','تقدر تغيّر الباركود الأساسي من الحقل التالي. عند تغييره، نحتفظ بالباركود السابق كبديل ولا نحذفه.':'You can change the primary barcode in the field below. The previous barcode is retained as an alternate instead of being deleted.','سيظهر هذا الباركود في قائمة المنتجات؛ لا يمكن استخدام باركود لصنف آخر.':'This barcode will appear in the product list. A barcode assigned to another product cannot be used.','الباركود الأساسي لا يمكن أن يكون فارغًا':'The primary barcode cannot be empty','العودة إلى الأصناف النشطة':'Back to active products','عرض الأصناف الموقوفة':'Show inactive products','تسجيل حركة الوارد والمنصرف':'Record stock movement','نوع الحركة':'Movement type','وارد — إضافة للمخزون':'Inbound — add to stock','منصرف — صرف من المخزون':'Outbound — remove from stock','باركود أو كود الصنف':'Product barcode or code','الكمية':'Quantity','ملاحظات (اختياري)':'Notes (optional)','تسجيل الحركة':'Record movement','مسح الحقول':'Clear fields','سجل حركات المخزون':'Stock movement history','حجم النسخة:':'Backup size:','إنشاء نسخة احتياطية إلى ملف':'Create backup file','استعادة من ملف خارجي':'Restore from file','تحديث نقاط الاستعادة':'Refresh restore points','نقاط الاستعادة المحفوظة داخل مدار':'Restore points saved in Madar','تأكيد استعادة البيانات':'Confirm data restore','موافق، استعد النسخة':'Confirm restore','جارٍ إنشاء حساب مدير النظام...':'Creating system administrator...','جارٍ التحقق من بيانات الدخول...':'Checking sign-in details...','تسجيل الدخول إلى مدار':'Sign in to Madar','إعداد مدار لأول مرة':'Set up Madar','إنشاء حساب المدير':'Create administrator account','اسم المدير الظاهر':'Administrator display name','كلمة المرور (12 حرفًا على الأقل)':'Password (at least 12 characters)','تم تسجيل الدخول':'Signed in successfully','إلغاء تحديد الكل':'Clear all','تحديد الكل':'Select all','إرجاع صلاحيات الدور':'Reset to role permissions','حفظ الصلاحيات':'Save permissions','تخصيص صلاحيات المستخدم':'Customize user permissions','لا تملك صلاحية لفتح هذا القسم.':'You do not have permission to open this section.','تعذر تحميل الأصناف:':'Could not load products:','فشل الحفظ:':'Save failed:','تعذر تحميل حسابات المستخدمين:':'Could not load user accounts:','تعذر إنشاء المستخدم:':'Could not create user:','تعذر حفظ بيانات المستخدم:':'Could not save user details:','تعذر تغيير دور المستخدم:':'Could not change user role:','تعذر تحديث حالة الحساب:':'Could not update account status:','تعذر تغيير كلمة المرور:':'Could not reset password:','تعذر تحميل سجل النشاط:':'Could not load activity log:','عدد السجلات المعروضة:':'Records displayed:','تم حفظ بيانات المستخدم':'User details saved','تم إنشاء حساب المستخدم':'User account created','تم حفظ الصلاحيات التفصيلية للمستخدم':'Detailed permissions saved for user','تم إيقاف المستخدم':'User deactivated','تم تفعيل المستخدم':'User activated','تم تغيير كلمة مرور':'Password changed for','تم حفظ الصنف':'Product saved','فشل تسجيل الدخول:':'Sign-in failed:','تعذر إنشاء حساب المدير:':'Could not create administrator:','تعذر فحص إعداد المستخدمين:':'Could not check user setup:','جارٍ تحميل سجل النشاط...':'Loading activity log...','لا توجد سجلات':'No records','كلمة المرور الحالية':'Current password','كلمة المرور الجديدة':'New password','حفظ كلمة المرور':'Save password','إضافة باركود بديل':'Add alternate barcode','إغلاق':'Close','تأكيد الإجراء':'Confirm action','نعم، نفّذ':'Yes, continue','إلغاء':'Cancel','نجح':'Succeeded','الإجراءات':'Actions','إدارة الأصناف':'Product management','إضافة الأصناف والبحث فيها وتعديلها وإيقافها بأمان.':'Add, search, edit and safely deactivate products.','أرصدة الأصناف':'Product balances','يعرض الرصيد الحالي لكل صنف، بما في ذلك الأصناف الموقوفة حتى لا تضيع أرصدتها من العرض.':'Current stock for every product, including inactive products.','بحث بالاسم أو الكود أو الباركود':'Search by name, code or barcode','بحث بالاسم أو الكود أو الباركود — استخدم قارئ الباركود هنا':'Search by name, code or barcode — scan here','الباركود يُعامل كنص للحفاظ على الأصفار في بدايته. عند التعديل لا يتغير الباركود الأساسي، ويمكن إضافة باركودات بديلة.':'Barcodes are stored as text to preserve leading zeros. Editing a product does not change its primary barcode; alternate barcodes can be added.','استخدم زر «إضافة صنف» لفتح نموذج مستقل، أو ابحث في القائمة وعدّل بيانات أي صنف.':'Use Add Product to open a separate form, or search and edit a product in the list.','تعديل الصنف':'Edit product','إدارة المخزون':'Inventory management','متابعة أرصدة الأصناف وسجل حركات الوارد والمنصرف. كل حركة تُحفظ في سجل مستقل، والرصيد محسوب من الحركات.':'Track stock balances and inbound/outbound movements. Each movement is stored separately; balances are calculated from movements.','استخدم «وارد» لإضافة كمية إلى الرصيد، و«منصرف» لصرف كمية موجودة. يمنع مدار صرف كمية أكبر من الرصيد المتاح، ولا يسمح بتسجيل حركة لصنف موقوف. تُحفظ وحدة القياس مع كل حركة؛ ولا يمكن تغيير وحدة صنف له رصيد حالي حتى لا تتغير دلالة الكميات.':'Use Inbound to add stock and Outbound to issue stock. Madar prevents issuing more than the available balance and blocks movements for inactive products. Each movement retains its unit; a unit cannot be changed while the item has stock.','تحديث المخزون':'Refresh inventory','الرصيد الحالي':'Current balance','الصنف':'Product','التاريخ':'Date','الكمية':'Quantity','الوحدة':'Unit','ملاحظات':'Notes','آخر 100 حركة مسجلة. الإشارة الموجبة تعني واردًا، والسالبة تعني منصرفًا.':'Latest 100 movements. Positive quantities are inbound; negative quantities are outbound.','النسخ الاحتياطي والاستعادة':'Backup and restore','إدارة نسخ قاعدة البيانات ونقاط الاستعادة بأمان.':'Safely manage database backups and restore points.','النسخ الاحتياطي واستعادة البيانات':'Backup and restore data','أنشئ نسخة احتياطية في المكان الذي تختاره. وعند الاستعادة، يحفظ مدار نسخة تلقائية من الوضع الحالي قبل التغيير، وتظهر هذه النسخ في قائمة نقاط الاستعادة بالأسفل حتى تتمكن من الرجوع إليها بضغطة واحدة.':'Create a backup wherever you choose. Before restoring, Madar saves the current database as a restore point so you can return to it later.','هذه نسخ الأمان التلقائية التي أنشأها مدار قبل عمليات الاستعادة السابقة، مرتبة من الأحدث إلى الأقدم.':'Automatic safety copies made before previous restores, newest first.','إضافة باركود بديل':'Add alternate barcode','أدخل الباركود الإضافي للصنف. سيظل الباركود الأساسي كما هو.':'Enter an additional barcode. The primary barcode will remain unchanged.','الباركودات لا تتكرر بين الأصناف. تقدر تضيف أكثر من باركود بديل للصنف نفسه.':'Barcodes must be unique across products. Multiple alternate barcodes are allowed for a product.','إعادة تفعيل صنف':'Reactivate product','تأكيد الإجراء':'Confirm action','حذف النسخة':'Delete restore point','استعادة هذه النسخة':'Restore this point','لا توجد عمليات مسجلة مطابقة للبحث.':'No matching activity was found.','لا توجد نقاط استعادة تلقائية حتى الآن. عند تنفيذ أول استعادة، سيحفظ مدار الوضع الحالي هنا تلقائيًا.':'No restore points yet. Madar will create one before the first restore.','قائمة نقاط الاستعادة تظهر عند تشغيل تطبيق مدار.':'Restore points are available when Madar is running.','جارٍ تحميل أرصدة المخزون...':'Loading inventory balances...','جارٍ تحميل الأصناف...':'Loading products...','جارٍ تحميل السجل...':'Loading activity...','جارٍ تحميل الصلاحيات...':'Loading permissions...','جارٍ تحميل حركات المخزون...':'Loading stock movements...','جارٍ تحميل نقاط الاستعادة...':'Loading restore points...','جارٍ تحميل المستخدمين...':'Loading users...','جارٍ تجهيز الصورة...':'Preparing image...','تم تجهيز الصورة.':'Image prepared.','تعذر تجهيز الصورة:':'Could not prepare image:','الصورة الشخصية':'Profile photo','إزالة الصورة':'Remove photo','حقل البريد الإلكتروني أطول من الحد المسموح':'Email address exceeds the allowed length','صيغة البريد الإلكتروني غير صحيحة':'Invalid email address format','الصورة كبيرة جدًا؛ اختر صورة أصغر من 500 كيلوبايت تقريبًا':'Image is too large. Choose an image around 500 KB or smaller.','صيغة الصورة غير مدعومة؛ استخدم JPG أو PNG أو WebP':'Unsupported image format. Use JPG, PNG or WebP.','تم إلغاء إنشاء النسخة الاحتياطية.':'Backup creation cancelled.','تم إلغاء الاستعادة.':'Restore cancelled.','فشلت الاستعادة:':'Restore failed:','فشل إنشاء النسخة الاحتياطية:':'Backup creation failed:','تم إلغاء حذف نقطة الاستعادة.':'Restore point deletion cancelled.','فشل حذف نقطة الاستعادة:':'Restore point deletion failed:','نجح إنشاء':'Created successfully','تسجيل دخول ناجح':'Successful login','محاولة دخول فاشلة':'Failed login attempt','تعديل بيانات مستخدم':'Update user profile','وارد مخزون':'Stock inbound','منصرف مخزون':'Stock outbound','إنشاء صنف':'Create product','تعديل صنف':'Edit product','إيقاف صنف':'Deactivate product','إضافة باركود بديل':'Add alternate barcode','إعادة تفعيل حساب مستخدم':'Reactivate user account','تم تعديل بيانات المستخدم':'User profile updated','الإنجليزية':'English','العربية':'Arabic','إدارة الأصناف':'Product management','الأصناف المسجلة':'Registered products','بحث بالاسم أو الكود أو الباركود':'Search by name, code or barcode','بحث بالاسم أو الكود أو الباركود — استخدم قارئ الباركود هنا':'Search by name, code or barcode — scan here','أرصدة الأصناف':'Product balances','الرصيد الحالي':'Current balance','إدارة المخزون':'Inventory management','تحديث المخزون':'Refresh inventory','آخر 100 حركة مسجلة. الإشارة الموجبة تعني واردًا، والسالبة تعني منصرفًا.':'Latest 100 movements. Positive amounts are inbound and negative amounts are outbound.','استخدم «وارد» لإضافة كمية إلى الرصيد، و«منصرف» لصرف كمية موجودة. يمنع مدار صرف كمية أكبر من الرصيد المتاح، ولا يسمح بتسجيل حركة لصنف موقوف. تُحفظ وحدة القياس مع كل حركة؛ ولا يمكن تغيير وحدة صنف له رصيد حالي حتى لا تتغير دلالة الكميات.':'Use Inbound to add stock and Outbound to issue stock. Madar prevents issuing more than the available balance and blocks movements for inactive products. Each movement retains its unit; a unit cannot be changed while the item has stock.','متابعة أرصدة الأصناف وسجل حركات الوارد والمنصرف. كل حركة تُحفظ في سجل مستقل، والرصيد محسوب من الحركات.':'Track stock balances and inbound/outbound movements. Each movement is stored separately; balances are calculated from movements.','اختيار القسم اللي عايز تشتغل فيه.':'Choose the section you want to work in.','أنشئ حساب مدير النظام. لا توجد كلمة مرور افتراضية؛ احفظ كلمة المرور في مكان آمن.':'Create a system administrator account. There is no default password; store the password safely.','سجّل الدخول بحسابك للوصول إلى الأقسام المسموح بها حسب صلاحياتك.':'Sign in to access sections allowed by your permissions.','إدارة نسخ قاعدة البيانات ونقاط الاستعادة بأمان.':'Safely manage database backups and restore points.','أنشئ نسخة احتياطية في المكان الذي تختاره. وعند الاستعادة، يحفظ مدار نسخة تلقائية من الوضع الحالي قبل التغيير، وتظهر هذه النسخ في قائمة نقاط الاستعادة بالأسفل حتى تتمكن من الرجوع إليها بضغطة واحدة.':'Create a backup wherever you choose. Before restoring, Madar saves the current database as a restore point so you can return to it later.','هذه نسخ الأمان التلقائية التي أنشأها مدار قبل عمليات الاستعادة السابقة، مرتبة من الأحدث إلى الأقدم.':'Automatic safety copies made before previous restores, newest first.','أدخل بيانات الصنف. لو تركت الباركود الأساسي فارغًا، سيولّد مدار باركودًا داخليًا تلقائيًا.':'Enter the product details. If the primary barcode is blank, Madar will generate an internal barcode.','أدخل الباركود الإضافي للصنف. سيظل الباركود الأساسي كما هو.':'Enter an additional barcode. The primary barcode will remain unchanged.','الباركودات لا تتكرر بين الأصناف. تقدر تضيف أكثر من باركود بديل للصنف نفسه.':'Barcodes must be unique across products. Multiple alternate barcodes are allowed for a product.','الباركود الداخلي لا يُعد رقم GTIN عالميًا ولا يُستخدم كبديل لباركود تجاري مسجل.':'The internal barcode is not a global GTIN and cannot replace a registered commercial barcode.','نحافظ على الباركود القديم إذا كان موجودًا وفريدًا.':'Existing unique barcodes are preserved.','لو مش موجود، يُنشأ باركود داخلي بصيغة MAD متوافق مع الطباعة عبر Code 128.':'If none exists, an internal MAD barcode is generated for Code 128 printing.','يمكن لاحقًا إضافة باركودات متعددة للصنف حسب العبوة أو الوحدة.':'Multiple barcodes can be assigned by package or unit.','البريد والهاتف والعنوان والصورة بيانات اختيارية.':'Email, phone, address, and photo are optional.','تعذر':'Could not','فشل':'Failed','فشلت':'Failed','خطأ':'Error','لا يمكنك':'You cannot','ليس لديك':'You do not have permission to','لا تملك':'You do not have permission to','لا توجد صلاحية':'No permission','لا يمكن':'Cannot','المستخدم غير موجود':'User not found','الحساب غير موجود':'Account not found','اسم المستخدم أو كلمة المرور':'Username or password','جارٍ تجهيز الصورة...':'Preparing image...','تم تجهيز الصورة.':'Image prepared.','تم حفظ بيانات المستخدم':'User profile saved for','تم إنشاء حساب المستخدم':'User account created for','تم تغيير كلمة مرور':'Password changed for','إيقاف الحساب':'Deactivate account','إعادة تفعيل الحساب':'Reactivate account','الصلاحيات':'Permissions','العنوان (اختياري)':'Address (optional)','مسح المرشحات':'Clear filters','المرشحات النشطة':'Active filters','رقم التليفون':'Phone number','الإنجليزية':'English','العربية':'Arabic','مدار © 2026 — نسخة تطوير أولية، يلزم بناؤها واختبارها قبل الاستخدام الفعلي.':'Madar © 2026 — development build. Build and test before production use.','المفتاح الأساسي':'Primary key','حالة قاعدة البيانات':'Database status','نسخة احتياطية':'Backup','قاعدة البيانات':'Database','الصورة كبيرة جدًا؛ اختر صورة أصغر من 500 كيلوبايت تقريبًا':'Image is too large. Choose an image around 500 KB or smaller.','استخدم صورة JPG أو PNG أو WebP فقط.':'Use a JPG, PNG, or WebP image.','حجم الصورة يجب ألا يتجاوز 5 ميجابايت.':'Image size must not exceed 5 MB.','تم تجهيز الصورة.':'Image prepared.','تعذر تجهيز الصورة:':'Could not prepare image:','فشلت الاستعادة:':'Restore failed:','فشل إنشاء النسخة الاحتياطية:':'Backup creation failed:','فشل حذف نقطة الاستعادة:':'Failed to delete restore point:','تعذر حفظ بيانات المستخدم:':'Could not save user profile:','تم إلغاء حذف نقطة الاستعادة.':'Restore point deletion cancelled.','تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.':'Restore cancelled. No safety backup was created.','عدد السجلات المعروضة:':'Records displayed:','آخر دخول':'Last login','حسب الدور':'Role defaults','استثناءات فردية':'custom overrides'
};
Object.assign(arEn,{
 'تم تحديث دور المستخدم':'User role updated for','تم تفعيل المستخدم':'User activated:','تم إيقاف المستخدم':'User deactivated:','تم حفظ الصلاحيات التفصيلية للمستخدم':'Detailed permissions saved for user','تم حفظ بيانات المستخدم':'User profile saved for','تم إنشاء حساب المستخدم':'User account created for','تم تغيير كلمة مرور':'Password changed for','تم تغيير كلمة المرور بنجاح':'Password changed successfully.','تم إضافة باركود بديل':'Alternate barcode added.','تم إيقاف الصنف':'Product deactivated.','تمت إعادة تفعيل الصنف':'Product reactivated.','تم حفظ الحركة':'Stock movement recorded.','تم إنشاء النسخة الاحتياطية':'Backup created.','تمت الاستعادة بنجاح':'Restore completed successfully.','تم إعداد النظام':'System setup completed.','تم تسجيل الدخول':'Signed in successfully.','تم تغيير دور المستخدم':'User role changed for','مسح المرشحات':'Clear filters','مخزون':'Inventory','كلمة المرور':'Password','تسجيل الدخول':'Sign in','＋ إضافة صنف':'+ Add product','الكود':'Code','الباركود الأساسي':'Primary barcode','شغّل التطبيق لتحميل البيانات.':'Run the app to load data.','اختر صنفًا لعرض رصيده الحالي.':'Choose a product to view its current balance.','الحركة':'Movement','جارٍ تحميل سجل الحركات...':'Loading movement history...','صنف':'Product','مستخدم':'User','جلسة':'Session','تسجيل خروج':'Log out','إنشاء مستخدم':'Create user','إيقاف حساب مستخدم':'Deactivate user account','تفعيل حساب مستخدم':'Activate user account','تغيير دور مستخدم':'Change user role','تخصيص صلاحيات مستخدم':'Customize user permissions','إعادة تعيين كلمة مرور':'Reset password','تغيير كلمة مرور':'Change password','حذف نقطة استعادة':'Delete restore point','تطبيق مكتبي محلي بقاعدة SQLite. كل صنف جديد يحصل على باركود أساسي تلقائيًا إذا لم تدخل باركودًا موجودًا.':'A local desktop app using SQLite. New products get an automatically generated primary barcode if none is provided.','سيتم الحفاظ على الباركود الأساسي الحالي.':'The current primary barcode will be preserved.','حفظ التعديلات':'Save changes','استخدم كلمة مرور قوية لا تقل عن 12 حرفًا.':'Use a strong password of at least 12 characters.','سجل النشاط والتدقيق':'Activity and audit log'
});
// Expanded localization coverage for previously untranslated static labels and dynamic operation messages.
Object.assign(arEn, {"مدار | نظام إدارة الأعمال":"Madar | Business Management System","م":"M","🌙 الوضع الداكن":"🌙 Dark mode","☀️ الوضع الفاتح":"☀️ Light mode","كلمة المرور":"Password","تسجيل الدخول":"Sign in","＋ إضافة صنف":"+ Add product","أنت تعرض الأصناف النشطة. الأصناف التي توقفها لا تُحذف، ويمكنك العثور عليها من زر «عرض الأصناف الموقوفة» وإعادة تفعيلها في أي وقت.":"You are viewing active products. Deactivated products are not deleted; find them using “Show inactive products” and reactivate them at any time.","أنت تعرض الأصناف الموقوفة. هذه الأصناف لم تُحذف؛ بياناتها وباركوداتها محفوظة. استخدم «إعادة تفعيل» لإرجاع الصنف إلى قائمة الأصناف النشطة.":"You are viewing inactive products. These products have not been deleted; their data and barcodes are preserved. Use “Reactivate” to return a product to the active list.","الكود":"Code","الباركود الأساسي":"Primary barcode","شغّل التطبيق لتحميل البيانات.":"Run the application to load data.","تسجيل حركة مخزون":"Record stock movement","اختر صنفًا لعرض رصيده الحالي.":"Select a product to view its current balance.","الحركة":"Movement","جارٍ تحميل سجل الحركات...":"Loading stock movement history...","سجل النشاط والتدقيق":"Activity and audit log","يعرض العمليات المهمة واسم المستخدم وتوقيتها. لا يسمح هذا القسم بتعديل السجل.":"This section shows important operations, the user who performed them, and timestamps. The log cannot be edited here.","صنف":"Product","مخزون":"Inventory","مستخدم":"User","جلسة":"Session","إنشاء مدير النظام":"Create system administrator","تسجيل خروج":"Log out","إنشاء مستخدم":"Create user","إيقاف حساب مستخدم":"Deactivate user account","تفعيل حساب مستخدم":"Activate user account","تغيير دور مستخدم":"Change user role","تخصيص صلاحيات مستخدم":"Customize user permissions","إعادة تعيين كلمة مرور":"Reset password","تغيير كلمة مرور":"Change password","إنشاء نسخة احتياطية":"Create backup","حذف نقطة استعادة":"Delete restore point","استعادة قاعدة البيانات":"Restore database","تطبيق مكتبي محلي بقاعدة SQLite. كل صنف جديد يحصل على باركود أساسي تلقائيًا إذا لم تدخل باركودًا موجودًا.":"A local desktop application using SQLite. Each new product receives an automatically generated primary barcode if you do not provide one.","سيتم الحفاظ على الباركود الأساسي الحالي.":"The current primary barcode will be preserved.","يتطلب صلاحية تعديل سعر الشراء.":"Requires permission to edit the purchase price.","يتطلب صلاحية تعديل سعر البيع.":"Requires permission to edit the sale price.","حفظ التعديلات":"Save changes","استخدم كلمة مرور قوية لا تقل عن 12 حرفًا.":"Use a strong password of at least 12 characters.","حدد بالضبط ما يستطيع هذا المستخدم تنفيذه. الصلاحيات هنا تتجاوز الافتراضات الموروثة من الدور، وتسري عند تسجيل الدخول التالي.":"Choose exactly what this user can do. These individual permissions override role defaults and take effect the next time the user signs in.","تبديل الوضع اللوني":"Toggle color theme","اسمك أو اسم المسؤول":"Your name or the administrator’s name","مثال: admin":"e.g. admin","امسح الباركود أو اكتب اسم الصنف / الكود":"Scan a barcode or enter the product name/code","امسح باركودًا أو اكتب اسم الصنف / الكود":"Scan a barcode or enter the product name/code","امسح الباركود ثم اضغط Enter":"Scan the barcode, then press Enter","مثال: 12 أو 1.250":"e.g. 12 or 1.250","مثال: رصيد افتتاحي أو توريد فاتورة رقم...":"e.g. opening balance or supply for invoice no...","مثال: P-1001":"e.g. P-1001","باركود موجود أو اتركه للتوليد":"Existing barcode, or leave blank to generate one","امسح الباركود أو اكتبه هنا":"Scan or type the barcode here","لا توجد أصناف موقوفة مطابقة للبحث.":"No inactive products match your search.","لا توجد أصناف نشطة مطابقة للبحث.":"No active products match your search.","إعادة تفعيل":"Reactivate","تعديل":"Edit","باركود بديل":"Alternate barcode","إيقاف":"Deactivate","باركود بديل — ":"Alternate barcode — ","الصنف: ":"Product: "," | الكود: ":" | Code: ",". أدخل باركودًا إضافيًا؛ لن يتغير الباركود الأساسي.":". Enter an additional barcode; the primary barcode will not change.","هل تريد إيقاف الصنف «":"Deactivate product “","»؟\n\nلن يتم حذف الصنف أو باركوداته. بعد التأكيد سينتقل إلى قائمة «الأصناف الموقوفة»، ويمكنك إعادة تفعيله لاحقًا.":"”?\n\nThe product and its barcodes will not be deleted. After confirmation, it will move to the inactive products list, where you can reactivate it later.","تأكيد إيقاف الصنف":"Confirm product deactivation","نعم، أوقف الصنف":"Yes, deactivate product","تم إلغاء العملية؛ لم يتم تغيير حالة الصنف.":"Operation cancelled; product status was not changed.","تم إيقاف «":"Deactivated “","». الصنف محفوظ الآن في قائمة الأصناف الموقوفة، ويمكنك إعادة تفعيله من هناك.":"”. The product is now in the inactive products list and can be reactivated there.","تعذر إيقاف الصنف:":"Could not deactivate product: ","هل تريد إعادة تفعيل الصنف «":"Reactivate product “","»؟  سيعود إلى قائمة الأصناف النشطة وستظل بياناته وباركوداته كما هي.":"”? It will return to the active products list with all data and barcodes preserved.","تأكيد إعادة تفعيل الصنف":"Confirm product reactivation","نعم، أعد التفعيل":"Yes, reactivate","تم إلغاء العملية؛ ظل الصنف موقوفًا.":"Operation cancelled; the product remains inactive.","تمت إعادة تفعيل «":"Reactivated “","» وإعادته إلى قائمة الأصناف النشطة.":"” and returned it to the active products list.","تعذرت إعادة تفعيل الصنف:":"Could not reactivate product: ","قطعة":"Piece","الحفظ متاح عند تشغيل التطبيق داخل Tauri.":"Saving is available when running the app in Tauri.","جارٍ حفظ الصنف...":"Saving product...","فشل حفظ الصنف:":"Failed to save product: ","إضافة الباركود متاحة عند تشغيل التطبيق داخل Tauri.":"Adding barcodes is available when running the app in Tauri.","أدخل الباركود البديل أولًا.":"Enter the alternate barcode first.","جارٍ حفظ الباركود البديل...":"Saving alternate barcode...","يمكنك إدخال باركود بديل آخر للصنف نفسه.":"You can add another alternate barcode for this product.","تعذرت إضافة الباركود البديل:":"Could not add alternate barcode: ","تم تعديل «":"Updated “","» مع الحفاظ على الباركود الأساسي: ":"” while preserving the primary barcode: ","فشل حفظ التعديلات:":"Failed to save changes: ","لا توجد أصناف تطابق البحث.":"No products match your search.","اختر الصنف":"Select a product","وارد":"Inbound","منصرف":"Outbound","لم تُسجّل حركات مخزون بعد.":"No stock movements have been recorded yet.","عرض المخزون متاح عند تشغيل تطبيق مدار داخل Tauri.":"Inventory is available when running Madar in Tauri.","عدد الأصناف المعروضة:":"Products displayed: ","تعذر تحميل المخزون:":"Could not load inventory: ","الرصيد الحالي للصنف «":"Current balance for “","»":"”"," — الصنف موقوف ولا يمكن تسجيل حركة له.":" — this product is inactive and cannot have stock movements recorded.","قراءة الباركود متاحة عند تشغيل التطبيق داخل Tauri.":"Barcode scanning is available when running the app in Tauri.","لم يتم العثور على صنف مطابق للباركود أو الكود. تأكد أن الصنف نشط.":"No product matches this barcode or code. Make sure the product is active.","تم تحديد الصنف: ":"Selected product: ","تعذر قراءة الصنف:":"Could not look up product: ","تسجيل حركات المخزون متاح عند تشغيل تطبيق مدار داخل Tauri.":"Stock movements are available when running Madar in Tauri.","اختر الصنف أولًا، ويمكنك مسح باركوده في الحقل المخصص.":"Select a product first, or scan its barcode in the designated field.","أدخل كمية أكبر من صفر.":"Enter a quantity greater than zero.","جارٍ تسجيل حركة المخزون...":"Recording stock movement...","تعذر تسجيل حركة المخزون:":"Could not record stock movement: ","قائمة نقاط الاستعادة تظهر عند تشغيل تطبيق مدار.":"Restore points are available when running Madar.","لا توجد نقاط استعادة تلقائية حتى الآن. عند تنفيذ أول استعادة، سيحفظ مدار الوضع الحالي هنا تلقائيًا.":"There are no automatic restore points yet. Madar will save the current state here before the first restore.","أحدث نقطة استعادة":"Latest restore point","نقطة استعادة ":"Restore point ","حجم النسخة: ":"Backup size: ","استعادة هذه النسخة":"Restore this backup","حذف النسخة":"Delete backup","هل تريد حذف نقطة الاستعادة التالية نهائيًا؟ ":"Permanently delete this restore point? ","  لن يمكن التراجع عن الحذف.":"  This action cannot be undone.","جارٍ حذف نقطة الاستعادة...":"Deleting restore point...","سيتم استبدال البيانات الحالية بمحتويات نقطة الاستعادة المحددة. بعد الضغط على موافق فقط، سيحفظ مدار الوضع الحالي كنقطة أمان ثم يبدأ الاستعادة. هل تريد المتابعة؟":"The current data will be replaced with the selected restore point. Only after you confirm will Madar save a safety backup of the current state and begin restoring. Continue?","جارٍ التحقق من نقطة الاستعادة وتطبيقها...":"Validating and applying restore point...","تم تحديث البيانات.":"Data refreshed.","سجّل الدخول مرة أخرى":"Sign in again","تعذر تحميل نقاط الاستعادة:":"Could not load restore points: ","ميزة اختيار الملفات متاحة عند تشغيل نسخة Tauri المبنية من المشروع.":"File selection is available when running the built Tauri app.","حفظ نسخة احتياطية من مدار":"Save a Madar backup","قاعدة بيانات SQLite":"SQLite database","جارٍ إنشاء النسخة الاحتياطية والتحقق منها...":"Creating and verifying backup...","اختيار نسخة مدار للاستعادة":"Choose a Madar backup to restore","تحذير: ستُستبدل البيانات الحالية بمحتويات النسخة المختارة. بعد الضغط على موافق فقط، سيحتفظ مدار بنسخة أمان من قاعدة البيانات الحالية ثم يبدأ الاستعادة. هل تريد المتابعة؟":"Warning: Current data will be replaced by the selected backup. Only after you confirm will Madar save a safety copy of the current database and begin restoring. Continue?","جارٍ التحقق من النسخة واستعادة البيانات...":"Validating backup and restoring data...","سيُعاد تحميل قائمة الأصناف الآن.":"The product list will now be reloaded.","لا تملك صلاحية تعديل سعر الشراء":"You do not have permission to edit purchase prices","لا تملك صلاحية تعديل سعر البيع":"You do not have permission to edit sale prices","ابدأ بإنشاء حساب مدير النظام. يتطلب إنشاء الحساب كلمة مرور من 12 حرفًا على الأقل.":"Create the system administrator account to get started. The password must contain at least 12 characters.","تعذر تجهيز الصورة":"Could not prepare image","الصورة أكبر من الحد المسموح بعد الضغط.":"The image exceeds the allowed size after compression.","تعذر قراءة الصورة المختارة.":"Could not read the selected image."," استثناءات فردية":" individual overrides","حسب الدور":"Role defaults","عرض التفاصيل":"View details","الصلاحيات":"Permissions","لا يوجد مستخدمون.":"No users found.","المستخدمون والحسابات":"Users and accounts","تخصيص الصلاحيات — ":"Customize permissions — ","صلاحيات أخرى":"Other permissions","تخصيص فردي":"Individual override","افتراضي الدور":"Role default","غير ممنوحة بالدور":"Not granted by role","مدير النظام فقط":"System administrator only","عدّل الاختيارات ثم احفظ. المنح الفردية تتجاوز افتراضات الدور، ويمكن إرجاع الاختيارات إلى إعدادات الدور.":"Update the selections and save. Individual grants override role defaults. You can reset the selections to the role defaults.","تعذر تحميل صلاحيات المستخدم:":"Could not load user permissions: ","جارٍ حفظ الصلاحيات...":"Saving permissions...","تعذر حفظ الصلاحيات:":"Could not save permissions: ","النظام/غير معروف":"System/Unknown","عرض المنتجات":"View products","عرض وبحث الأصناف والباركودات":"View and search products and barcodes","إدارة المنتجات (قديم)":"Manage products (legacy)","صلاحية شاملة قديمة لإدارة الأصناف":"Legacy full permission to manage products","عرض المخزون":"View inventory","عرض الأرصدة وسجل الحركات":"View balances and movement history","إدارة المخزون (قديم)":"Manage inventory (legacy)","صلاحية شاملة قديمة لحركات المخزون":"Legacy full permission for stock movements","النسخ والاستعادة (قديم)":"Backup and restore (legacy)","صلاحية شاملة قديمة للنسخ والاستعادة":"Legacy full permission for backup and restore","إدارة المستخدمين (قديم)":"Manage users (legacy)","صلاحية شاملة قديمة لإدارة المستخدمين":"Legacy full permission for user administration","عرض سجل النشاط":"View activity log","عرض عمليات المستخدمين وسجل التدقيق":"View user actions and audit log","إضافة صنف":"Add product","إنشاء أصناف جديدة":"Create new products","تعديل بيانات الصنف":"Edit product details","تعديل الكود والاسم والوحدة والبيانات العامة":"Edit code, name, unit, and general details","إيقاف صنف":"Deactivate product","إيقاف الصنف دون حذفه نهائيًا":"Deactivate a product without permanently deleting it","إعادة تفعيل صنف":"Reactivate product","إرجاع الصنف الموقوف إلى الأصناف النشطة":"Return an inactive product to the active list","إدارة الباركودات":"Manage barcodes","إضافة باركودات بديلة للصنف":"Add alternate barcodes for a product","تعديل سعر الشراء":"Edit purchase price","تغيير سعر الشراء عند إنشاء أو تعديل صنف":"Change the purchase price when creating or editing a product","تعديل سعر البيع":"Edit sale price","تغيير سعر البيع عند إنشاء أو تعديل صنف":"Change the sale price when creating or editing a product","تسجيل حركات المخزون":"Record stock movements","تسجيل الوارد والمنصرف يدويًا":"Manually record inbound and outbound movements","عرض نقاط الاستعادة":"View restore points","عرض نقاط الاستعادة المحفوظة داخل مدار":"View restore points saved by Madar","حفظ نسخة احتياطية إلى ملف":"Save a backup to a file","استعادة نسخة احتياطية":"Restore backup","استعادة قاعدة البيانات من نسخة":"Restore the database from a backup","حذف نقاط الاستعادة التي أنشأها مدار":"Delete restore points created by Madar","عرض المستخدمين":"View users","عرض قائمة حسابات المستخدمين":"View the list of user accounts","تعديل بيانات المستخدم":"Edit user profile","تعديل البريد والهاتف والعنوان والصورة والاسم الظاهر":"Edit email, phone, address, photo, and display name","إنشاء حساب مستخدم جديد":"Create a new user account","تفعيل وإيقاف الحسابات":"Activate and deactivate accounts","تفعيل أو إيقاف حساب المستخدم":"Activate or deactivate a user account","تغيير أدوار المستخدمين":"Change user roles","تغيير دور المستخدم":"Change a user role","تخصيص صلاحيات المستخدم":"Customize user permissions","ضبط الصلاحيات الفردية لكل مستخدم؛ لمدير النظام فقط":"Set individual permissions for each user; system administrator only","إعادة تعيين كلمة المرور":"Reset password","تعيين كلمة مرور جديدة لمستخدم آخر":"Set a new password for another user","جارٍ إنشاء حساب مدير النظام...":"Creating system administrator account...","جارٍ التحقق من بيانات الدخول...":"Verifying sign-in details...","جارٍ تسجيل الخروج...":"Signing out...","تم إلغاء إنشاء النسخة الاحتياطية.":"Backup creation cancelled.","تم إلغاء الاستعادة.":"Restore cancelled.","تعذر تحميل الأصناف:":"Could not load products: ","تعذر تحميل حسابات المستخدمين:":"Could not load user accounts: ","تعذر إنشاء حساب المدير:":"Could not create administrator account: ","تعذر فحص إعداد المستخدمين:":"Could not check user setup: ","فشل تسجيل الدخول:":"Sign-in failed: ","تم إلغاء حذف نقطة الاستعادة.":"Restore point deletion cancelled.","تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.":"Restore cancelled. No safety backup was created.","تعذر تغيير كلمة المرور:":"Could not change password: ","تعذر تحديث حالة الحساب:":"Could not update account status: ","تعذر تغيير دور المستخدم:":"Could not change user role: ","تعذر حفظ بيانات المستخدم:":"Could not save user profile: ","تعذر إنشاء المستخدم:":"Could not create user: ","فشل حذف نقطة الاستعادة:":"Failed to delete restore point: ","فشل إنشاء النسخة الاحتياطية:":"Failed to create backup: ","فشلت الاستعادة:":"Restore failed: ","تم حفظ بيانات المستخدم":"User profile saved","تم إنشاء حساب المستخدم":"User account created","تم إيقاف المستخدم":"User deactivated","تم تفعيل المستخدم":"User activated","تم تحديث دور المستخدم":"User role updated","تم حفظ الصلاحيات التفصيلية للمستخدم":"Detailed permissions saved for user","تم إضافة باركود بديل":"Alternate barcode added","تم حفظ الصنف":"Product saved","تم إنشاء النسخة الاحتياطية":"Backup created","تمت الاستعادة بنجاح":"Restore completed successfully","تم تسجيل الدخول":"Signed in successfully","تم حفظ الحركة":"Stock movement recorded","هل تريد إعادة تفعيل الحساب ":"Reactivate account ","؟":"?","هل تريد إيقاف الحساب ":"Deactivate account ","؟ لن يستطيع تسجيل الدخول بعد ذلك.":"? The user will no longer be able to sign in.","نعم، فعّل الحساب":"Yes, activate account","نعم، أوقف الحساب":"Yes, deactivate account","إدارة حساب مستخدم":"Manage user account","لا توجد عمليات مسجلة مطابقة للبحث.":"No matching activity records found.","المعرف":"ID","المعرّف":"ID","التاريخ":"Date","التاريخ والوقت":"Date and time","الملاحظات":"Notes"});

Object.assign(arEn, {"لن يتم حذف الصنف أو باركوداته. بعد التأكيد سينتقل إلى قائمة «الأصناف الموقوفة»، ويمكنك إعادة تفعيله لاحقًا.":"The product and its barcodes will not be deleted. After confirmation, it will move to the inactive products list, where you can reactivate it later.","سيعود إلى قائمة الأصناف النشطة وستظل بياناته وباركوداته كما هي.":"It will return to the active products list with all data and barcodes preserved.","لن يمكن التراجع عن الحذف.":"This action cannot be undone.","الأصناف الموقوفة":"inactive products","الأصناف النشطة":"active products","الباركودات محفوظة":"barcodes are preserved","تم إلغاء إنشاء النسخة الاحتياطية.":"Backup creation cancelled.","تم إلغاء الاستعادة.":"Restore cancelled.","تم إلغاء العملية":"Operation cancelled","نقطة الاستعادة":"restore point","نقاط الاستعادة":"restore points","حجم النسخة":"Backup size","حفظ نسخة أمان":"save a safety backup","البيانات الحالية":"current data","النسخة المختارة":"selected backup","الصنف محفوظ الآن في قائمة الأصناف الموقوفة":"The product is now saved in the inactive products list","تفعيل الحساب":"Activate account","إيقاف الحساب":"Deactivate account","تعذر تحميل الأصناف:":"Could not load products: ","تعذر تحميل حسابات المستخدمين:":"Could not load user accounts: ","تعذر فحص إعداد المستخدمين:":"Could not check user setup: ","تم إعداد النظام":"System setup completed","تعذر تعديل":"Could not update","تعذر حفظ":"Could not save","تم الحفظ":"Saved","تعذر إنشاء":"Could not create","نجح":"Succeeded","نجحت":"Succeeded","مرفوض":"Rejected","غير صالح":"Invalid","اسم المستخدم أو كلمة المرور غير صحيحة":"Incorrect username or password","الحساب موقوف":"The account is inactive","الحساب غير نشط":"The account is inactive","كلمة المرور الحالية غير صحيحة":"The current password is incorrect","كلمة المرور قصيرة جدًا":"The password is too short","حجم الصورة":"Image size","عنوان البريد الإلكتروني":"Email address","يجب":"Must","من فضلك":"Please","إدخال":"Enter"});

Object.assign(arEn, {"ليس لديك صلاحية لفتح هذا القسم.":"You do not have permission to open this section.","واجهة المعاينة فقط: افتح التطبيق داخل Tauri لتفعيل حفظ البيانات.":"Preview mode only: open the app in Tauri to enable data saving.","ميجابايت":"MB","تعذر الوصول إلى جلسة المستخدم":"Could not access the user session","يلزم تسجيل الدخول أولًا":"You must sign in first","ليس لديك صلاحية لتنفيذ هذه العملية ({permission})":"You do not have permission to perform this operation ({permission})","المستخدم غير موجود أو موقوف":"The user does not exist or is inactive","تعذر تأمين كلمة المرور: ":"Could not secure password: ","اسم المستخدم يجب أن يكون من 3 إلى 40 حرفًا إنجليزيًا أو رقمًا، ويمكن استخدام النقطة والشرطة والشرطة السفلية":"Username must be 3–40 characters and use English letters or numbers; dots, hyphens, and underscores are also allowed","حقل ":"Field "," أطول من الحد المسموح (":" exceeds the allowed length ("," حرفًا)":" characters)","صيغة البريد الإلكتروني غير صحيحة":"Invalid email address format","صيغة الصورة غير مدعومة؛ استخدم JPG أو PNG أو WebP":"Unsupported image format. Use JPG, PNG, or WebP","تعذر قراءة جلسة المستخدم":"Could not read the user session","اكتب اسم مدير النظام":"Enter the system administrator name","كلمة المرور لازم تكون 12 حرفًا على الأقل":"Password must contain at least 12 characters","تم إنشاء حساب بالفعل؛ استخدم شاشة تسجيل الدخول":"An account already exists. Use the sign-in screen","تعذر إنشاء الحساب؛ تأكد أن اسم المستخدم غير مستخدم: ":"Could not create account. Make sure the username is not already in use: ","تعذر إنشاء جلسة المستخدم":"Could not create the user session","اسم المستخدم أو كلمة المرور غير صحيحة، أو أن الحساب موقوف":"Incorrect username or password, or the account is inactive","بيانات الدخول غير صحيحة":"Invalid sign-in details","اكتب الاسم الظاهر للمستخدم":"Enter the user display name","الاسم الظاهر طويل جدًا (الحد 120 حرفًا)":"Display name is too long (maximum 120 characters)","الدور المحدد غير صالح":"The selected role is invalid","إنشاء مدير نظام جديد متاح لمدير النظام فقط":"Only the system administrator can create another system administrator","تعذر إنشاء المستخدم؛ تأكد أن اسم المستخدم غير مستخدم: ":"Could not create user. Make sure the username is not already in use: ","تم إنشاء حساب المستخدم ":"User account created: ","الاسم الظاهر مطلوب":"Display name is required","المستخدم غير موجود":"User not found","تم حفظ بيانات المستخدم ":"User profile saved: ","لا يمكنك إيقاف حسابك أثناء استخدامه":"You cannot deactivate your own account while signed in","إدارة حسابات مدير النظام متاحة لمدير النظام فقط":"Only the system administrator can manage system administrator accounts","لا يمكن إيقاف آخر مدير نظام نشط":"The last active system administrator cannot be deactivated","تم تفعيل المستخدم ":"User activated: ","تم إيقاف المستخدم ":"User deactivated: ","كلمة المرور الجديدة لازم تكون 12 حرفًا على الأقل":"The new password must contain at least 12 characters","إعادة تعيين كلمة مرور مدير النظام متاحة لمدير النظام فقط":"Only the system administrator can reset an administrator password","تم تغيير كلمة مرور ":"Password changed for ","الحساب غير موجود أو موقوف":"The account does not exist or is inactive","بيانات كلمة المرور المحفوظة غير صالحة":"The stored password data is invalid","كلمة المرور الحالية غير صحيحة":"The current password is incorrect","اختر كلمة مرور جديدة مختلفة عن الحالية":"Choose a new password that differs from the current password","تم تغيير كلمة المرور بنجاح":"Password changed successfully","لأمان النظام، لا يمكن تغيير دور الحساب المستخدم حاليًا. استخدم حساب مدير آخر عند الحاجة.":"For system security, you cannot change the role of the account currently in use. Use another administrator account if needed.","إدارة دور مدير النظام متاحة لمدير النظام فقط":"Only the system administrator can change an administrator role","لا يمكن تغيير دور آخر مدير نظام نشط":"The last active system administrator role cannot be changed","تعيين دور مدير النظام متاح لمدير النظام فقط":"Only the system administrator can assign the system administrator role","تم تحديث دور المستخدم ":"User role updated: ","محرر الصلاحيات متاح لمدير النظام فقط":"The permissions editor is available only to the system administrator","لا يمكن تخصيص صلاحيات حسابك الحالي؛ لتجنب فقدان الوصول للنظام":"You cannot customize permissions for your own account, to avoid losing system access","لا يمكن تخصيص صلاحيات حساب مدير النظام؛ فهو يحتفظ بكل الصلاحيات":"The system administrator account cannot be customized; it retains all permissions","تخصيص الصلاحيات متاح لمدير النظام فقط":"Only the system administrator can customize permissions","قائمة الصلاحيات تحتوي على تكرار غير متوقع":"The permission list contains an unexpected duplicate","الصلاحية غير معروفة: ":"Unknown permission: ","صلاحية تخصيص الصلاحيات لا يمكن منحها لمستخدم آخر؛ وهي محجوزة لمدير النظام":"The permission-management privilege cannot be granted to another user; it is reserved for the system administrator","صلاحية ":"Permission "," تتطلب أيضًا صلاحية ":" also requires permission ","صلاحيات تعديل الأسعار تتطلب السماح بإضافة الأصناف أو تعديل بياناتها":"Price-editing permissions also require permission to create or edit products","تم حفظ الصلاحيات التفصيلية للمستخدم ":"Detailed permissions saved for ","كود الصنف واسم الصنف مطلوبان":"Product code and name are required","السعر لا يمكن أن يكون سالبًا":"Price cannot be negative","الباركود مستخدم بالفعل لصنف آخر. أدخل باركود مختلفًا.":"This barcode is already used by another product. Enter a different barcode.","الصنف غير موجود أو نشط بالفعل":"Product not found or already active","تمت إعادة تفعيل الصنف مع الاحتفاظ ببياناته وباركوداته":"Product reactivated; its data and barcodes were preserved","الكود والاسم والوحدة حقول مطلوبة":"Code, name, and unit are required","الصنف غير موجود أو موقوف":"Product not found or inactive","لا تملك صلاحية تعديل سعر الشراء":"You do not have permission to edit the purchase price","لا تملك صلاحية تعديل سعر البيع":"You do not have permission to edit the sale price","لا يمكن تغيير وحدة الصنف إلى «":"Cannot change the product unit to “","» لأن له رصيدًا حاليًا. صفّر الرصيد أو عالج تحويل الوحدة أولًا حتى لا تتغير دلالة الكميات المسجلة.":"” because it has a current balance. Clear the balance or handle the unit conversion first so recorded quantities retain their meaning.","تعذر تعديل الصنف (تأكد أن الكود غير مستخدم): ":"Could not update product (make sure the code is not already in use): ","أدخل الباركود البديل أولًا":"Enter the alternate barcode first","هذا الباركود مستخدم بالفعل، ولن يتم ربطه بصنف آخر":"This barcode is already in use and will not be linked to another product","تمت إضافة الباركود البديل ":"Alternate barcode added: ","الصنف غير موجود أو موقوف بالفعل":"Product not found or already inactive","تم إيقاف الصنف مع الاحتفاظ ببياناته وباركوداته":"Product deactivated; its data and barcodes were preserved","الكمية لازم تكون أكبر من صفر":"Quantity must be greater than zero","نوع الحركة غير صالح؛ اختر وارد أو منصرف":"Invalid movement type. Choose inbound or outbound","الصنف غير موجود أو موقوف؛ أعد تفعيله قبل تسجيل حركة جديدة":"Product not found or inactive. Reactivate it before recording a new movement","الرصيد المتاح للصنف «":"Available balance for “","» هو ":"” is "," فقط؛ لا يمكن صرف ":" only; cannot issue ","تم تسجيل ":"Recorded "," للصنف «":" for product “","» بكمية ":"” with quantity ","لم يتم تحديد مكان حفظ النسخة الاحتياطية":"No destination was selected for the backup","لا يمكن حفظ النسخة فوق قاعدة البيانات الأصلية":"The backup cannot overwrite the original database","مجلد الحفظ غير موجود":"The destination folder does not exist","فشل التحقق من النسخة: ":"Backup validation failed: ","تعذر تحديد مجلد قاعدة البيانات":"Could not determine the database directory","مسار نقطة الاستعادة غير صالح":"Invalid restore-point path","يمكن حذف نقاط الاستعادة المحفوظة داخل مجلد مدار فقط":"Only restore points stored in Madar’s restore folder can be deleted","اسم ملف غير صالح":"Invalid file name","هذا الملف ليس نقطة استعادة معتمدة للحذف":"This file is not an approved restore point for deletion","نقطة الاستعادة غير موجودة بالفعل":"The restore point does not exist","تعذر حذف نقطة الاستعادة: ":"Could not delete restore point: ","تم حذف نقطة الاستعادة: ":"Restore point deleted: ","ملف النسخة الاحتياطية غير موجود":"Backup file not found","اختر ملف نسخة احتياطية مختلفًا عن قاعدة البيانات الحالية":"Choose a backup file different from the current database","تعذر فتح النسخة الاحتياطية: ":"Could not open backup: ","ملف النسخة غير صالح: ":"Backup file is invalid: ","النسخة الاحتياطية غير سليمة: ":"Backup is corrupted: ","الملف المحدد لا يبدو نسخة من قاعدة بيانات مدار":"The selected file does not appear to be a Madar database backup","فشل التحقق من النسخة المستعادة: ":"Restored backup validation failed: ","تعذر إرفاق قاعدة البيانات الحالية للحفاظ على المستخدمين والسجل: ":"Could not attach the current database to preserve users and audit history: ","تعذر الحفاظ على حسابات المستخدمين وسجل التدقيق أثناء الاستعادة: ":"Could not preserve user accounts and audit history during restore: ","تعذر حفظ قاعدة البيانات الحالية قبل الاستعادة: ":"Could not save the current database before restoring: ","تعذر تفعيل النسخة المستعادة، وتمت محاولة إعادة قاعدة البيانات السابقة: ":"Could not activate the restored backup; an attempt was made to restore the previous database: ","تعذر إنهاء الجلسة بعد الاستعادة":"Could not end the session after restore","تمت الاستعادة. احتفظنا بقاعدة البيانات السابقة في: ":"Restore completed. The previous database was saved at: ",". سجّل الدخول مرة أخرى.":". Sign in again.","خطأ أثناء تشغيل مدار":"Error while starting Madar","استخدم صورة JPG أو PNG أو WebP فقط.":"Use a JPG, PNG, or WebP image only.","حجم الصورة يجب ألا يتجاوز 5 ميجابايت.":"Image size must not exceed 5 MB.","الصورة كبيرة جدًا؛ اختر صورة أصغر من 500 كيلوبايت تقريبًا":"Image is too large. Choose an image smaller than about 500 KB.","تم تحديد الصنف: ":"Selected product: ","الرصيد الحالي للصنف ":"Current balance for product ","جارٍ تحميل الأصناف...":"Loading products...","جارٍ تحميل أرصدة المخزون...":"Loading inventory balances...","جارٍ تحميل نقاط الاستعادة...":"Loading restore points...","جارٍ تحميل السجل...":"Loading activity log...","حفظ بيانات المستخدم":"Save user profile","إدارة المنتجات":"Manage products","إدارة المخزون":"Manage inventory","النظام":"System"});

Object.assign(arEn, {"الوارد":"Inbound","المنصرف":"Outbound","حركة الوارد":"inbound movement","حركة المنصرف":"outbound movement","تم تسجيل الوارد":"Inbound movement recorded","تم تسجيل المنصرف":"Outbound movement recorded"});

Object.assign(arEn, {"رقم الهاتف":"Phone number",". تسري التغييرات عند تسجيل دخوله التالي.":". Changes take effect the next time the user signs in."});

Object.assign(arEn, {
"العملاء والموردون":"Customers & suppliers",
"إدارة بيانات الاتصال، والعناوين، والملاحظات. إيقاف السجل لا يحذف تاريخه أو الفواتير المرتبطة به.":"Manage contact details, addresses, and notes. Deactivating a record does not delete its history or linked invoices.",
"العملاء":"Customers","الموردون":"Suppliers","العميل":"Customer","المورد":"Supplier",
"إضافة عميل":"Add customer","إضافة مورد":"Add supplier","＋ إضافة عميل":"＋ Add customer","＋ إضافة مورد":"＋ Add supplier",
"تعديل العميل":"Edit customer","تعديل المورد":"Edit supplier","تحديث القائمة":"Refresh list","بحث":"Search",
"ابحث بالكود أو الاسم أو البريد الإلكتروني أو رقم الهاتف.":"Search by code, name, email, or phone.",
"كود أو اسم أو بريد أو هاتف":"Code, name, email, or phone","عدد السجلات:":"Records:","لا توجد سجلات مطابقة.":"No matching records.",
"نشط":"Active","موقوف":"Inactive","تعديل":"Edit","حفظ":"Save","إلغاء":"Cancel","إغلاق":"Close","إضافة":"Add",
"تمت إضافة السجل: ":"Record added: ","تم تحديث البيانات: ":"Record updated: ","تعذر تحميل السجلات:":"Could not load records:","تعذر حفظ السجل:":"Could not save record:","تعذر تغيير حالة السجل:":"Could not change record status:",
"سجل جهة الاتصال غير موجود":"Contact record not found","حالة جهة الاتصال لم تتغير":"Contact status did not change","نوع جهة الاتصال غير صالح":"Invalid contact type",
"تعذر تعديل البيانات (تأكد أن الكود غير مستخدم): ":"Could not update record (make sure the code is not already in use): ","تعذر إنشاء السجل (تأكد أن الكود غير مستخدم): ":"Could not create record (make sure the code is not already in use): ",
"هل تريد تفعيل":"Do you want to activate","هل تريد إيقاف":"Do you want to deactivate","لن يتم حذف البيانات أو الفواتير المرتبطة.":"The data and linked invoices will not be deleted.","نعم، فعّل":"Yes, activate","نعم، أوقف":"Yes, deactivate",
"فواتير المشتريات":"Purchase invoices","سجل فواتير المشتريات":"Purchase invoice register","فواتير المبيعات":"Sales invoices","سجل فواتير المبيعات":"Sales invoice register",
"احفظ الفاتورة كمسودة، ثم اعتمدها عند المراجعة. الاعتماد يضيف الكميات للمخزون داخل معاملة واحدة.":"Save invoices as drafts, then post after review. Posting adds quantities to inventory in one transaction.",
"لا يتغير المخزون بمجرد إنشاء المسودة؛ يتم تحديثه فقط عند الاعتماد.":"Creating a draft does not change inventory; stock is updated only when it is posted.",
"أنشئ فاتورة بيع، وراجعها قبل الاعتماد. يعتمد مدار الفاتورة فقط إذا كانت أرصدة المخزون كافية.":"Create a sales invoice and review it before posting. Madar posts only when sufficient stock is available.",
"اختيار العميل اختياري للبيع النقدي. اعتماد الفاتورة يخصم الكميات المتاحة.":"Selecting a customer is optional for a cash sale. Posting deducts the available quantities.",
"＋ فاتورة مشتريات جديدة":"＋ New purchase invoice","＋ فاتورة مبيعات جديدة":"＋ New sales invoice","فاتورة مشتريات جديدة":"New purchase invoice","فاتورة مبيعات جديدة":"New sales invoice",
"رقم الفاتورة":"Invoice no.","التاريخ":"Date","الأصناف":"Items","الإجمالي":"Total","الحالة":"Status","أنشأها":"Created by","الإجراءات":"Actions","تفاصيل":"Details","اعتماد":"Post","تعديل المسودة":"Edit draft",
"لا توجد فواتير مطابقة.":"No matching invoices.","عدد الفواتير المعروضة:":"Invoices displayed:","مسودة":"Draft","معتمدة":"Posted","ملغاة":"Cancelled","عميل نقدي":"Cash customer",
"تاريخ الفاتورة":"Invoice date","خصم الفاتورة":"Invoice discount","أصناف الفاتورة":"Invoice items","＋ إضافة صنف":"＋ Add item","الصنف":"Product","الكمية":"Quantity","سعر الوحدة":"Unit price",
"الإجمالي قبل الخصم:":"Subtotal before discount:","الإجمالي النهائي:":"Final total:","حفظ المسودة":"Save draft","حفظ التعديلات":"Save changes",
"المسودة لا تغير المخزون إلا عند اعتمادها. بعد الاعتماد لا يمكن تعديلها، ويجب الإلغاء لعكس أثرها.":"Drafts do not affect inventory until posted. Posted invoices cannot be edited; cancel them to reverse the effect.",
"المسودة لا تغير المخزون إلا عند اعتمادها.":"Drafts do not affect inventory until posted.","تم حفظ المسودة":"Draft saved","تم تحديث المسودة":"Draft updated","بإجمالي":"with total",
"تعذر تحميل البيانات اللازمة للفاتورة:":"Could not load data required for the invoice:","أضف صنفًا واحدًا على الأقل للفاتورة.":"Add at least one item to the invoice.","راجع الصنف والكمية والسعر في كل سطر.":"Check the product, quantity, and price on every line.","تعذر حفظ الفاتورة:":"Could not save invoice:","تعذر تحميل الفواتير:":"Could not load invoices:",
"تفاصيل الفاتورة":"Invoice details","جارٍ تحميل التفاصيل...":"Loading details...","تعذر تحميل تفاصيل الفاتورة:":"Could not load invoice details:",
"الفاتورة غير موجودة":"Invoice not found","يمكن اعتماد المسودات فقط؛ حالة الفاتورة الحالية لا تسمح بالاعتماد":"Only draft invoices can be posted.","لا يمكن اعتماد فاتورة بدون أصناف":"Cannot post an invoice without items",
"لا يمكن تعديل الفاتورة إلا إذا كانت مسودة.":"Only draft invoices can be edited.","يمكن تعديل المسودات فقط؛ الفاتورة المعتمدة أو الملغاة لا تتغير":"Only drafts can be edited; posted or cancelled invoices cannot be changed",
"المورد غير موجود أو موقوف":"Supplier not found or inactive","العميل غير موجود أو موقوف":"Customer not found or inactive","تاريخ الفاتورة يجب أن يكون بصيغة YYYY-MM-DD":"Invoice date must use YYYY-MM-DD format","تاريخ الفاتورة غير صالح":"Invoice date is invalid",
"أضف صنفًا واحدًا على الأقل للفاتورة":"Add at least one item to the invoice","لا يمكن أن تتجاوز الفاتورة 200 سطر":"An invoice cannot contain more than 200 lines","الفاتورة يجب أن تحتوي من صنف واحد إلى 200 صنف":"An invoice must contain between 1 and 200 items",
"الخصم لا يمكن أن يكون سالبًا":"Discount cannot be negative","سعر السطر لا يمكن أن يكون سالبًا":"Line price cannot be negative","كل كمية يجب أن تكون أكبر من صفر":"Each quantity must be greater than zero","راجع الكميات والأسعار في الفاتورة":"Check quantities and prices on the invoice",
"قيمة السطر أكبر من الحد المسموح":"Line value exceeds the permitted limit","إجمالي الفاتورة أكبر من الحد المسموح":"Invoice total exceeds the permitted limit","خصم الفاتورة لا يمكن أن يتجاوز الإجمالي":"Invoice discount cannot exceed subtotal",
"لا تملك صلاحية تجاوز سعر البيع المسجل للصنف ":"You do not have permission to override the registered sale price for product ","إنشاء الفاتورة يتطلب صلاحية عرض الفواتير والأصناف":"Creating an invoice requires permission to view invoices and products",
"تحتاج صلاحية عرض الموردين لإنشاء فاتورة مشتريات":"Permission to view suppliers is required to create a purchase invoice","تحتاج صلاحية عرض العملاء لإنشاء فاتورة مبيعات":"Permission to view customers is required to create a sales invoice",
"لازم تملك صلاحية عرض الفواتير قبل اعتمادها":"Invoice-view permission is required before posting","لازم تملك صلاحية عرض الفواتير قبل إلغائها":"Invoice-view permission is required before cancellation",
"تم اعتماد الفاتورة ":"Invoice posted ","وتحديث المخزون بنجاح":"and inventory updated successfully","وتحديث المخزون بنجاح":"and inventory updated successfully","تعذر اعتماد الفاتورة:":"Could not post invoice:",
"هل تريد إلغاء الفاتورة ":"Cancel invoice ","سيظل سجل الفاتورة محفوظًا.":"The invoice record will be retained.","سيتم اعتماد الفاتورة ":"Post invoice ","سيضيف ذلك الكميات إلى المخزون.":"This will add quantities to inventory.","سيتم التحقق من المخزون وخصم الكميات.":"Stock will be checked and quantities deducted.","لا يمكن تعديل الفاتورة بعد الاعتماد؛ الإلغاء سيُسجل كحركة عكسية.":"Posted invoices cannot be edited; cancellation is recorded as a reversing movement.",
"سيتم إلغاء المسودة دون تغيير المخزون.":"The draft will be cancelled without changing inventory.","سيحاول النظام عكس الكميات الواردة، وقد يرفض الإلغاء إذا كانت الكمية لم تعد متاحة بالمخزون.":"The system will reverse inbound quantities and may reject cancellation if the stock is no longer available.","سيعيد النظام كميات الفاتورة إلى المخزون.":"The system will return the invoice quantities to inventory.","تعذر إلغاء الفاتورة:":"Could not cancel invoice:","تم إلغاء الفاتورة ":"Invoice cancelled ",". تم الحفاظ على سجلها":". The invoice record was retained","وعكس أثرها على المخزون بحركات موثقة":"and its inventory effect was reversed through recorded movements","كمسودة دون حركة مخزون":"as a draft without stock movements","الفاتورة ملغاة بالفعل":"The invoice is already cancelled",
"تعديل المسودة يتطلب صلاحية عرض الفواتير وإنشائها":"Editing a draft requires permission to view and create invoices","تعديل الفاتورة يتطلب صلاحية عرض الأصناف":"Editing an invoice requires permission to view products","تحتاج صلاحية عرض الموردين لتعديل فاتورة مشتريات":"Permission to view suppliers is required to edit a purchase invoice","تحتاج صلاحية عرض العملاء لتعديل فاتورة مبيعات":"Permission to view customers is required to edit a sales invoice",
"تم إنشاء الفاتورة لكن تعذر تحميلها":"Invoice created, but could not reload it","تعذر تحميل المسودة المعدلة":"Could not reload the updated draft","لا يمكن تغيير وحدة الصنف مع وجود فواتير مسودة تستخدمه؛ عدّل أو ألغِ المسودات أولًا.":"A product unit cannot be changed while a draft invoice uses it. Edit or cancel the drafts first."
});


Object.assign(arEn,{
 'المشتريات':'Purchases','المبيعات':'Sales','العملاء':'Customers','الموردون':'Suppliers','العميل':'Customer','المورد':'Supplier',
 'مسح باركود الصنف':'Scan item barcode','امسح الباركود ثم اضغط Enter لإضافة الصنف':'Scan the barcode, then press Enter to add the item','يمكن استخدام قارئ الباركود الذي يعمل كلوحة مفاتيح.':'You can use a barcode scanner that acts as a keyboard.',
 'لم يتم العثور على صنف نشط بالباركود':'No active product found for barcode','تمت إضافة':'Added','إلى الفاتورة.':'to the invoice.','تعذر البحث عن الباركود:':'Could not look up barcode:',
 'تحتاج صلاحية تجاوز سعر البيع لتغيير السعر في الفاتورة':'The sales price override permission is required to change this invoice price',
 'إنشاء مسودة مشتريات':'Purchase draft created','تعديل مسودة مشتريات':'Purchase draft updated','اعتماد مشتريات':'Purchase invoice posted','إلغاء مشتريات':'Purchase invoice cancelled',
 'إنشاء مسودة مبيعات':'Sales draft created','تعديل مسودة مبيعات':'Sales draft updated','اعتماد مبيعات':'Sales invoice posted','إلغاء مبيعات':'Sales invoice cancelled',
 'عميل نقدي':'Cash customer','فاتورة مشتريات جديدة':'New purchase invoice','فاتورة مبيعات جديدة':'New sales invoice','حفظ المسودة':'Save draft','حفظ التعديلات':'Save changes','المسودة لا تغير المخزون إلا عند اعتمادها.':'Drafts do not change stock until posted.',
 'أصناف الفاتورة':'Invoice items','إضافة صنف':'Add item','إزالة':'Remove','الكمية':'Quantity','سعر الوحدة':'Unit price','الإجمالي قبل الخصم:':'Subtotal before discount:','الإجمالي النهائي:':'Final total:','الخصم':'Discount','تاريخ الفاتورة':'Invoice date','ملاحظات':'Notes',
 'تم تحديث المسودة':'Draft updated','تم حفظ المسودة':'Draft saved','أضف صنفًا واحدًا على الأقل للفاتورة.':'Add at least one item to the invoice.','راجع الصنف والكمية والسعر في كل سطر.':'Check the item, quantity and price on each line.','تعذر حفظ الفاتورة:':'Could not save invoice:',
 'لا يمكن اعتماد فاتورة بدون أصناف':'An invoice without items cannot be posted','الرصيد غير كافٍ للصنف':'Insufficient stock for item','يمكن اعتماد المسودات فقط؛ حالة الفاتورة الحالية لا تسمح بالاعتماد':'Only draft invoices can be posted','الفاتورة ملغاة بالفعل':'Invoice is already cancelled','تم اعتماد الفاتورة':'Invoice posted','تم إلغاء الفاتورة':'Invoice cancelled',
 'تفاصيل':'Details','تعديل المسودة':'Edit draft','اعتماد':'Post','إلغاء':'Cancel','مسودة':'Draft','معتمدة':'Posted','ملغاة':'Cancelled','أنشأها':'Created by','رقم الفاتورة:':'Invoice number:','الحالة:':'Status:','الإجمالي:':'Total:','لا توجد فواتير مطابقة.':'No matching invoices.','تعذر تحميل الفواتير:':'Could not load invoices:',
 'لا يمكن تعديل الفاتورة إلا إذا كانت مسودة.':'Only draft invoices can be edited.','المسودة لا تغير المخزون إلا عند اعتمادها. بعد الاعتماد لا يمكن تعديلها، ويجب الإلغاء لعكس أثرها.':'Drafts do not affect stock until posted. Posted invoices cannot be edited; cancel to reverse their effect.',
 'تحتاج صلاحية عرض الموردين لإنشاء فاتورة مشتريات':'Supplier-view permission is required to create a purchase invoice','تحتاج صلاحية عرض العملاء لإنشاء فاتورة مبيعات':'Customer-view permission is required to create a sales invoice','أضف صنفًا واحدًا على الأقل للفاتورة':'Add at least one item to the invoice','الخصم لا يمكن أن يكون سالبًا':'Discount cannot be negative','خصم الفاتورة لا يمكن أن يتجاوز الإجمالي':'Invoice discount cannot exceed the subtotal',
 'إنشاء فاتورة مشتريات':'Create purchase invoice','إنشاء فاتورة مبيعات':'Create sales invoice','سجل فواتير المشتريات':'Purchase invoice register','سجل فواتير المبيعات':'Sales invoice register','مخزون داخل معاملة واحدة.':'stock in one transaction.',
 'إدارة بيانات الاتصال، والعناوين، والملاحظات. إيقاف السجل لا يحذف تاريخه أو الفواتير المرتبطة به.':'Manage contact details, addresses and notes. Deactivating a record does not delete its history or linked invoices.',
 'إدارة بيانات الاتصال، والعناوين، والملاحظات.':'Manage contact details, addresses and notes.','إيقاف السجل لا يحذف تاريخه أو الفواتير المرتبطة به.':'Deactivating a record does not delete its history or linked invoices.',
 'بيع نقدي بدون عميل':'Cash sale (no customer)','اختر المورد':'Choose supplier','لا توجد أصناف.':'No items.','الإجمالي قبل الخصم':'Subtotal before discount','التاريخ':'Date','الوحدة':'Unit'
});

Object.assign(arEn,{
 'إيقاف عميل':'Deactivate customer','إيقاف مورد':'Deactivate supplier','تعديل عميل':'Edit customer','تعديل مورد':'Edit supplier','تفعيل عميل':'Activate customer','تفعيل مورد':'Activate supplier',
 'الاسم':'Name','تحديث':'Refresh','جارٍ تحميل السجلات...':'Loading records...','جارٍ تحميل الفواتير...':'Loading invoices...','عملاء':'Customers','موردون':'Suppliers','فاتورة جديدة':'New invoice','فاتورة مبيعات':'Sales invoice','فاتورة مشتريات':'Purchase invoice',
 'المسودة لا تغير المخزون إلا بعد الاعتماد.':'Draft invoices do not affect inventory until posted.',
 'حفظ الفاتورة ينشئ مسودة فقط. من سجل الفواتير اختر «اعتماد» لتحديث المخزون وتسجيل العملية.':'Saving the invoice creates a draft only. Choose “Post” in the invoice register to update inventory and record the operation.',
 'رقم الفاتورة أو اسم العميل أو الملاحظات':'Invoice number, customer name or notes','رقم الفاتورة أو اسم المورد أو الملاحظات':'Invoice number, supplier name or notes'
});

Object.assign(arEn,{
 'المورد المرتبط بالفاتورة موقوف؛ أعد تفعيله قبل الاعتماد':'The supplier linked to this invoice is inactive. Reactivate it before posting.',
 'العميل المرتبط بالفاتورة موقوف؛ أعد تفعيله قبل الاعتماد':'The customer linked to this invoice is inactive. Reactivate them before posting.',
 'فاتورة المشتريات تحتاج إلى مورد':'A purchase invoice requires a supplier.'
});
Object.assign(arEn,{
 'عروض الأسعار':'Quotations','عرض سعر جديد':'New quotation','عرض سعر':'Quotation','سجل عروض الأسعار':'Quotation register','أنشئ عرض سعر للعميل واطبعه، ثم حدّث حالته أو حوّله إلى مسودة فاتورة مبيعات بعد قبوله.':'Create and print customer quotations, then update their status or convert accepted quotations to sales invoice drafts.',
 'عرض السعر لا يغيّر المخزون ولا ينشئ مستحقًا ماليًا حتى تحويله إلى فاتورة.':'A quotation does not change stock or create an account balance until converted to an invoice.',
 'رقم العرض':'Quotation No.','صالح حتى':'Valid until','إرسال':'Send','قبول':'Accept','رفض':'Reject','تحويل لمبيعات':'Convert to sale','تحويل عرض السعر':'Convert quotation','تم إرسال':'Sent','مقبول':'Accepted','مرفوض':'Rejected','محوّل':'Converted','منتهي':'Expired','ملغي':'Cancelled',
 'المدفوعات والخزنة':'Payments & Cashbook','تسجيل دفعة':'Record payment','نوع الفاتورة':'Invoice type','تحصيل من عميل — مبيعات':'Customer receipt — sales','سداد لمورد — مشتريات':'Supplier payment — purchases','الفاتورة المفتوحة':'Open invoice','مبلغ الدفعة':'Payment amount','تاريخ الدفعة':'Payment date','طريقة الدفع':'Payment method','نقدي':'Cash','بنك':'Bank','بطاقة':'Card','تحويل':'Transfer','أخرى':'Other','رقم مرجعي (اختياري)':'Reference (optional)','آخر الدفعات':'Recent payments','الاتجاه':'Direction','تحصيل':'Receipt','سداد':'Payment','الخزنة النقدية':'Cashbox','الرصيد النقدي الحالي':'Current cash balance','تسوية نقدية يدوية':'Manual cash adjustment','إضافة نقدية':'Cash in','صرف نقدية':'Cash out','البيان':'Description','تسجيل التسوية':'Record adjustment','سجل الخزنة':'Cashbook ledger','المبلغ هنا يحسب النقدية فقط؛ التحويلات البنكية والبطاقات لا تدخل في رصيد الخزنة.':'This balance counts cash only; bank transfers and card payments are excluded.',
 'التقارير':'Reports','من تاريخ':'From date','إلى تاريخ':'To date','عرض التقرير':'Run report','إجمالي المبيعات المعتمدة':'Posted sales total','إجمالي المشتريات المعتمدة':'Posted purchases total','تحصيلات العملاء':'Customer receipts','مدفوعات الموردين':'Supplier payments','متبقي على العملاء':'Customer receivables','متبقي للموردين':'Supplier payables','الربح الإجمالي التقديري':'Estimated gross profit','أصناف منخفضة المخزون':'Low-stock products',
 'بيانات الشركة':'Company profile','تظهر هذه البيانات في فواتير المبيعات والمشتريات وعروض الأسعار المطبوعة.':'These details appear on printed sales invoices, purchase invoices and quotations.',
 'اسم الشركة / النشاط':'Company / business name','رقم الهاتف':'Phone number','الموقع الإلكتروني':'Website','الرقم الضريبي':'Tax number','السجل التجاري':'Commercial registration','العملة':'Currency','الشعار (PNG/JPG/WebP)':'Logo (PNG/JPG/WebP)','معاينة الشعار':'Logo preview','إزالة الشعار':'Remove logo','التذييل المطبوع':'Printed footer','حفظ بيانات الشركة':'Save company profile','يمكنك عرض بيانات الشركة، ولا تملك صلاحية تعديلها.':'You can view the company details but do not have permission to edit them.',
 'طباعة':'Print','تفاصيل':'Details','الفواتير المعتمدة فقط':'Posted invoices only','اختر الفاتورة':'Choose invoice','المتبقي':'Balance due','تم تسجيل الدفعة بقيمة':'Payment recorded for','تعذر تحميل الفواتير المفتوحة':'Could not load open invoices','تعذر تجهيز الفاتورة للطباعة':'Could not prepare invoice for printing','تعذر طباعة عرض السعر':'Could not print quotation','لا توجد دفعات مسجلة.':'No payments recorded.','لا توجد حركات خزنة.':'No cashbook entries.','لا توجد عروض أسعار.':'No quotations found.','تاريخ صلاحية العرض غير صحيح':'Quotation validity date is invalid','يجب قبول عرض السعر قبل تحويله إلى فاتورة بيع':'The quotation must be accepted before conversion to a sales invoice.',
 'النسخة المطبوعة تستخدم بيانات الشركة المحفوظة في الإعدادات.':'Printed documents use the company details saved in settings.',
 'الربح الإجمالي التقديري':'Estimated gross profit','تنبيه: ':'Notice: ','لا تشمل ضرائب منفصلة.':'does not include separately calculated taxes.'
});


Object.assign(arEn,{
 'دفعة':'Payment','الخزنة':'Cashbox','بيانات الشركة':'Company profile','تاريخ العرض':'Quotation date','المدفوع':'Paid','المتبقي':'Balance due',
 'إنشاء عرض سعر':'Create quotation','تعديل عرض سعر':'Edit quotation','إرسال العرض':'Send quotation','قبول العرض':'Accept quotation','رفض العرض':'Reject quotation','تحويل عرض سعر إلى فاتورة':'Convert quotation to invoice',
 'إنشاء عرض سعر':'Create quotation','تعديل عرض سعر':'Update quotation','تغيير حالة عرض سعر':'Change quotation status','تسجيل دفعة':'Record payment','تسوية الخزنة':'Cashbox adjustment','تعديل بيانات الشركة':'Update company profile',
 'تم حفظ بيانات الشركة لاستخدامها في الفواتير والمطبوعات':'Company details saved for invoices and printed documents',
 'ليس لديك صلاحية عرض بيانات الشركة':'You do not have permission to view company details','لا تملك صلاحية عرض فواتير هذا النوع':'You do not have permission to view invoices of this type',
 'قيمة الدفعة يجب أن تكون أكبر من صفر':'Payment amount must be greater than zero','المبلغ يجب أن يكون أكبر من صفر':'Amount must be greater than zero',
 'طريقة الدفع غير صالحة':'Payment method is invalid','تاريخ الدفعة غير صالح':'Payment date is invalid','تاريخ الحركة غير صالح':'Cashbook entry date is invalid',
 'لا يمكن إلغاء فاتورة عليها دفعات مسجلة. عالج رد المبلغ أو تسوية الدفعة أولًا حتى لا يختل رصيد الحساب والخزنة.':'An invoice with recorded payments cannot be cancelled until the payment is refunded or reconciled.',
 'يمكن تسجيل دفعات على الفواتير المعتمدة فقط':'Payments can only be recorded against posted invoices','الدفعة أكبر من المتبقي على الفاتورة. المتبقي':'Payment exceeds the outstanding invoice balance. Remaining',
 'الرصيد النقدي لا يكفي لهذه التسوية. الرصيد الحالي':'The cash balance is insufficient for this adjustment. Current balance',
 'تحتاج صلاحيات إنشاء وعرض فواتير المبيعات':'Sales invoice create and view permissions are required',
 'يجب قبول عرض السعر قبل تحويله إلى فاتورة بيع':'The quotation must be accepted before converting it to a sales invoice',
 'عرض السعر غير موجود':'Quotation not found','يمكن تعديل مسودات عروض الأسعار فقط':'Only draft quotations can be edited',
 'الحالة الحالية لا تسمح بالاعتماد':'The current status does not allow posting',
 'لم يتم تحديد شركة':'No company has been configured',
 'الربح الإجمالي التقديري':'Estimated gross profit','تم تسجيل حركة الخزنة':'Cashbook entry recorded','تم تسجيل الدفعة بقيمة':'Payment recorded for',
 'تعذر تسجيل الدفعة:':'Could not record payment:','تعذر تسجيل التسوية:':'Could not record adjustment:','تعذر إنشاء التقرير:':'Could not generate report:',
 'الفترة:':'Period:','عدد عروض الأسعار:':'Quotations:','لا توجد عمليات مسجلة مطابقة للبحث.':'No audit events match the filters.',
 'خصم الفاتورة':'Invoice discount','تاريخ العرض':'Quotation date','صالح حتى':'Valid until'
});


Object.assign(arEn,{
 'التفضيلات':'Preferences','تفضيلات العرض':'Display preferences','اللغة والمظهر':'Language and appearance','الشركة المستخدمة':'Company','Change language':'Change language','تبديل الوضع اللوني':'Toggle color theme','⚙️':'⚙️',
 'ملخص سريع لأهم مؤشرات العمل واختصارات حسب صلاحيات حسابك.':'A quick summary of key business metrics and shortcuts available to your role.',
 'إجمالي الأصناف النشطة':'Total active products','أصناف برصيد أكبر من صفر':'Products with a positive balance','تحتاج مراجعة وإعادة طلب':'Needs review and replenishment','الفواتير المعتمدة من أول الشهر':'Posted invoices since the start of the month','الفواتير المعتمدة من أول الشهر':'Posted invoices since the start of the month',
 'أرصدة فواتير المبيعات':'Outstanding sales invoices','أرصدة فواتير المشتريات':'Outstanding purchase invoices','بانتظار المتابعة':'Awaiting follow-up','الدور الحالي':'Current role','تظهر الأقسام المتاحة حسب صلاحيات المستخدم.':'Sections are shown according to your permissions.',
 'الأصناف منخفضة المخزون':'Low-stock products','مبيعات الشهر':'Sales this month','مشتريات الشهر':'Purchases this month','مستحقات العملاء':'Customer receivables','مستحقات الموردين':'Supplier payables','عروض الأسعار المفتوحة':'Open quotations',
 'المنتجات':'Products','الأصناف والباركودات':'Products and barcodes','الأرصدة والوارد والمنصرف':'Balances, receipts and issues','جهات الاتصال والحسابات':'Contacts and accounts','إنشاء ومتابعة الفواتير':'Create and manage invoices','التوريد وإضافة المخزون':'Purchasing and stock receipt','إنشاء العروض ومتابعتها':'Create and track quotations','التحصيلات والمستحقات':'Receipts and outstanding balances','ملخص مالي وحركة العمل':'Financial and activity overview','حماية واستعادة البيانات':'Protect and restore data',
 'حساباتك':'Your account','صورة المستخدم':'User avatar','مدير النظام':'System administrator','مدير':'Manager','مسؤول مخزن':'Warehouse operator','مبيعات':'Sales','عرض فقط':'Read only','@':'@',
 'قاعدة بيانات محلية':'Local database','قاعدة البيانات محلية، ويتم التحقق من الصلاحيات داخل أوامر التطبيق. أنشئ نسخة احتياطية بصورة دورية.':'The database is local, and permissions are checked inside application commands. Create regular backups.',
 '🏢':'🏢','🟢':'🟢','📦':'📦','🏬':'🏬','⚠️':'⚠️','🧾':'🧾','🛒':'🛒','👥':'👥','💼':'💼','🏷️':'🏷️','📈':'📈','💳':'💳','💾':'💾','🛡️':'🛡️','📋':'📋','❔':'❔','🏠':'🏠'
});

Object.assign(arEn,{
 'بياناتي الشخصية':'My profile','عدّل بيانات التواصل والصورة الخاصة بحسابك. لا يمكن تغيير الدور أو الصلاحيات من هنا.':'Edit your contact details and avatar. Your role and permissions cannot be changed here.',
 'الصورة الشخصية':'Profile picture','استخدام الصورة الافتراضية':'Use default avatar','الاسم الظاهر':'Display name','اسم المستخدم':'Username','البريد الإلكتروني':'Email address','رقم التليفون':'Phone number','العنوان':'Address','حفظ بياناتي':'Save my profile',
 'جارٍ حفظ بياناتك الشخصية…':'Saving your profile…','تم تجهيز الصورة.':'Image prepared.','تعذر تجهيز الصورة:':'Could not prepare image:','تعذر حفظ بياناتك الشخصية:':'Could not save your profile:'
});

function localizeText(value){
 const trimmed=String(value??'').trim();if(currentLanguage!=='en'||!trimmed)return value;
 if(arEn[trimmed])return String(value).replace(trimmed,arEn[trimmed]);
 let m=trimmed.match(/^عدد السجلات المعروضة:\s*(\d+)(.*)$/);if(m)return `Records displayed: ${m[1]}${m[2].replace('المرشحات النشطة','active filters')}`;
 m=trimmed.match(/^تم حفظ الصنف:\s*(.+)\nالباركود:\s*(.+)$/);if(m)return `Product saved: ${m[1]}\nBarcode: ${m[2]}`;
 m=trimmed.match(/^تم حفظ الصلاحيات التفصيلية للمستخدم\s+(.+?)\. تسري التغييرات عند تسجيل دخوله التالي\.$/);if(m)return `Saved detailed permissions for user ${m[1]}. Changes take effect the next time the user signs in.`;
 m=trimmed.match(/^تم حفظ الصلاحيات التفصيلية للمستخدم\s+(.+)$/);if(m)return `Saved detailed permissions for user ${m[1]}`;
 m=trimmed.match(/^تم إيقاف المستخدم\s+(.+)$/);if(m)return `Deactivated user: ${m[1]}`;
 m=trimmed.match(/^تم تفعيل المستخدم\s+(.+)$/);if(m)return `Activated user: ${m[1]}`;
 let result=String(value);
 const translations=Object.entries(arEn).sort((a,b)=>b[0].length-a[0].length);
 for(const [source,target] of translations){
  if(!source||!result.includes(source))continue;
  // Match full words for single-token entries, so e.g. صنف never corrupts الصنف
  // and user-visible Arabic words are not replaced in the middle of other words.
  if(/^[A-Za-z0-9_\u0600-\u06FF]+$/.test(source)){
   const escaped=source.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
   const re=new RegExp('(^|[^A-Za-z0-9_\u0600-\u06FF])('+escaped+')(?=$|[^A-Za-z0-9_\u0600-\u06FF])','g');
   result=result.replace(re,(_match,before)=>before+target);
  }else result=result.split(source).join(target);
 }
 return result;
}
Object.assign(arEn,{
 'نسبة الضريبة (%)':'Tax rate (%)','طريقة احتساب الضريبة':'Tax calculation mode','الأسعار غير شاملة الضريبة (تضاف إلى الإجمالي)':'Prices exclude tax (added to total)','الأسعار شاملة الضريبة':'Prices include tax','تُطبّق هذه الإعدادات على الفواتير وعروض الأسعار الجديدة، وتُحفظ نسبة الضريبة وطريقتها مع كل مستند. المستندات القديمة لا يُعاد احتسابها.':'These settings apply to new invoices and quotations. The tax rate and mode are saved with each document. Existing documents are not recalculated.','الضريبة:':'Tax:','الضريبة المتضمنة في السعر':'Tax included in prices','الضريبة المضافة':'Tax added','سعر الوحدة (شامل الضريبة)':'Unit price (tax included)','سعر الوحدة (غير شامل الضريبة)':'Unit price (tax excluded)','تم حفظ بيانات الشركة وإعدادات الضريبة':'Company and tax settings saved','نسبة الضريبة يجب أن تكون رقمًا بين 0 و100%':'Tax rate must be a number between 0 and 100%','نسبة الضريبة يجب أن تكون بين 0 و100%':'Tax rate must be between 0 and 100%','إعداد شمول الأسعار للضريبة غير صالح':'The tax-inclusive price setting is invalid','ليس لديك صلاحية عرض بيانات الشركة':'You do not have permission to view company settings','ضريبة المبيعات':'Sales tax','ضريبة المشتريات':'Purchase tax','تم حفظ بيانات الشركة وإعدادات الضريبة':'Company and tax settings saved','نسبة الضريبة (%)':'Tax rate (%)','طريقة احتساب الضريبة':'Tax calculation mode','الأسعار غير شاملة الضريبة (تضاف إلى الإجمالي)':'Prices exclude tax (added to total)','الأسعار شاملة الضريبة':'Prices include tax','الضريبة المتضمنة في السعر':'VAT included in prices','الضريبة المضافة':'VAT added','الضريبة':'Tax'
});
function translateNode(node){
 if(!node||node.nodeType!==Node.TEXT_NODE||!node.parentElement||['SCRIPT','STYLE'].includes(node.parentElement.tagName)||(node.parentElement.id==='languageToggle'||node.parentElement.id==='authLanguageToggle')||node.parentElement.closest('[data-preserve-translation]'))return;
 if(node.__madarOriginalText===undefined)node.__madarOriginalText=node.nodeValue;
 const original=node.__madarOriginalText;const next=currentLanguage==='en'?localizeText(original):original;
 if(node.nodeValue!==next)node.nodeValue=next;
}
function translateDom(root=document){
 const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let node;while((node=walker.nextNode()))translateNode(node);
 const attrMap=['placeholder','title','aria-label'];const elements=root.querySelectorAll?root.querySelectorAll('[placeholder],[title],[aria-label]'):[];
 for(const el of elements){el.__madarOriginalAttrs=el.__madarOriginalAttrs||{};for(const attr of attrMap){if(!el.hasAttribute(attr))continue;if(el.__madarOriginalAttrs[attr]===undefined)el.__madarOriginalAttrs[attr]=el.getAttribute(attr);const original=el.__madarOriginalAttrs[attr];const next=currentLanguage==='en'?localizeText(original):original;if(el.getAttribute(attr)!==next)el.setAttribute(attr,next);}}
}
function applyLanguage(lang){
 currentLanguage=lang==='en'?'en':'ar';document.documentElement.lang=currentLanguage;document.documentElement.dir=currentLanguage==='en'?'ltr':'rtl';
 try{localStorage.setItem('madar-language',currentLanguage);}catch(_){}
 const button=document.getElementById('languageToggleLabel');if(button)button.textContent=currentLanguage==='en'?'العربية':'English';const authButton=document.getElementById('authLanguageToggle');if(authButton)authButton.textContent=currentLanguage==='en'?'العربية':'English';
 const brandMark=document.querySelector('.mark');if(brandMark)brandMark.textContent=currentLanguage==='en'?'M':'م';translateDom(document);const unitField=document.getElementById('unit');if(unitField&&(unitField.value==='قطعة'||unitField.value==='Piece'))unitField.value=currentLanguage==='en'?'Piece':'قطعة';applyTheme(document.documentElement.dataset.theme||'light');if(currentUser){renderHeaderUser(currentUser);refreshDashboard();}document.title=currentLanguage==='en'?'Madar | Business Management System':'مدار | نظام إدارة الأعمال';
}
onElement('languageToggle','click',()=>{applyLanguage(currentLanguage==='ar'?'en':'ar');closeHeaderMenus();});
onElement('authLanguageToggle','click',()=>applyLanguage(currentLanguage==='ar'?'en':'ar'));
const uiObserver=new MutationObserver(records=>{for(const rec of records){if(rec.type==='characterData')translateNode(rec.target);for(const added of rec.addedNodes||[]){if(added.nodeType===Node.TEXT_NODE)translateNode(added);else if(added.nodeType===Node.ELEMENT_NODE)translateDom(added);}}styleStatusMessages();});
function statusOriginalText(el){
 const parts=[];const walker=document.createTreeWalker(el,NodeFilter.SHOW_TEXT);let node;
 while((node=walker.nextNode()))parts.push(node.__madarOriginalText!==undefined?node.__madarOriginalText:node.nodeValue||'');
 return parts.join('').trim();
}
function styleStatusMessages(){
 document.querySelectorAll('.status,.auth-status').forEach(el=>{
  const visible=(el.textContent||'').trim();
  const original=statusOriginalText(el);
  // Classify against both the source-language message and its displayed translation.
  // That prevents color behavior from changing when the UI language changes.
  const candidates=[original,visible].filter(Boolean);
  const errorPattern=/^(?:تعذر|فشل(?:ت)?|خطأ|ليس لديك|لا يمكنك|لا تملك|لا توجد صلاحية|لا يمكن|لا تستطيع|المستخدم غير موجود|الحساب غير موجود|اسم المستخدم أو كلمة المرور|كلمة المرور الحالية غير|صيغة البريد|صيغة الصورة|حجم الصورة|الدور المحدد غير صالح|مرفوض|غير صالح|لم يتم|لم تُ|لم تتم|Could not\b|Cannot\b|Can’t\b|Failed\b|Failure\b|Error\b|No permission\b|Permission denied\b|User not found\b|Account not found\b|Username or password\b|You do not\b|You don't\b|You cannot\b|You can't\b|Invalid\b|Forbidden\b|Unable to\b|Incorrect\b|Not found\b|The .* (?:failed|is invalid|was rejected|could not)\b|Please (?:enter|select|choose|provide)\b)/i;
  const successPattern=/^(?:تم(?:ت|\s)|نجح(?:ت)?|حُفظ|اكتمل|تمت العملية|Successfully\b|Success(?:ful)?\b|Saved\b|Created\b|Updated\b|Deleted\b|Activated\b|Deactivated\b|Password changed\b|User profile saved\b|User account created\b|User activated\b|User deactivated\b|User role updated\b|User role changed\b|Detailed permissions saved\b|Backup created\b|Stock movement recorded\b|Product saved\b|Product reactivated\b|Product deactivated\b|Restore completed\b|Signed in successfully\b|Records displayed:\b|Image prepared\b|Invoice (?:posted|saved|created|cancelled)\b|Company and tax settings saved\b|Recorded\b|Added\b|Removed\b|Settings saved\b|Profile updated\b|Data saved\b|Quotation (?:created|updated|accepted)\b)/i;
  const error=candidates.some(t=>errorPattern.test(t));
  const success=!error&&candidates.some(t=>successPattern.test(t));
  el.classList.toggle('status-error',error);
  el.classList.toggle('status-success',success);
  if(error){el.setAttribute('aria-invalid','true');el.setAttribute('data-status-kind','error');}
  else if(success){el.removeAttribute('aria-invalid');el.setAttribute('data-status-kind','success');}
  else {el.removeAttribute('aria-invalid');el.removeAttribute('data-status-kind');}
 });
}
uiObserver.observe(document.body,{subtree:true,childList:true,characterData:true});
applyLanguage(currentLanguage);styleStatusMessages();


initializeAuth();
