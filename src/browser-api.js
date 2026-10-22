/* Browser adapter for preview/testing. Desktop keeps the real Tauri backend. */
(function () {
  const nativeInvoke = window.__TAURI__?.core?.invoke;
  const nativeDialog = window.__TAURI__?.dialog;
  const KEY = 'madar-browser-demo-v1';
  const defaults = { users: [], products: [], nextProductId: 1, session: null, restorePoints: [] };
  const read = () => { try { return { ...defaults, ...(JSON.parse(localStorage.getItem(KEY) || '{}')) }; } catch { return { ...defaults }; } };
  const write = data => localStorage.setItem(KEY, JSON.stringify(data));
  const permissions = ['dashboard.view','products.view','products.create','products.edit','products.deactivate','products.reactivate','products.barcode.manage','stock.view','stock.manage','backup.view_points','users.view','audit.view'];
  const userShape = u => ({ id:u.id, username:u.username, displayName:u.displayName, roleCode:'admin', roleName:'مدير النظام', permissions, email:null, phone:null, address:null, avatarDataUrl:null });
  function browserInvoke(command, args={}) {
    const db=read();
    switch(command) {
      case 'auth_status': return Promise.resolve({setupRequired:db.users.length===0,currentUser:db.session?userShape(db.users.find(x=>x.id===db.session)||db.users[0]):null});
      case 'create_initial_admin': {
        if(db.users.length) return Promise.reject('تم إنشاء حساب بالفعل في وضع المتصفح');
        if(!args.username||!args.password||String(args.password).length<4) return Promise.reject('أدخل اسم مستخدم وكلمة مرور من 4 أحرف على الأقل');
        const u={id:1,username:String(args.username).trim(),displayName:String(args.displayName||args.username).trim(),password:String(args.password)};db.users.push(u);db.session=u.id;write(db);return Promise.resolve(userShape(u));
      }
      case 'login': { const u=db.users.find(x=>x.username.toLowerCase()===String(args.username||'').toLowerCase()&&x.password===String(args.password||''));if(!u)return Promise.reject('اسم المستخدم أو كلمة المرور غير صحيحة');db.session=u.id;write(db);return Promise.resolve(userShape(u)); }
      case 'logout': db.session=null;write(db);return Promise.resolve(true);
      case 'app_info': return Promise.resolve({name:'Madar Browser Preview',version:'0.1.0',mode:'browser-demo'});
      case 'get_company_branding': return Promise.resolve({name:'',currencyCode:'EGP'});
      case 'get_dashboard_inventory_counts': return Promise.resolve({productCount:db.products.filter(p=>p.is_active!==false).length,stockCount:0});
      case 'list_products': case 'list_inactive_products': {const inactive=command==='list_inactive_products';return Promise.resolve(db.products.filter(p=>(p.is_active===false)===inactive));}
      case 'search_products': case 'search_inactive_products': {const inactive=command==='search_inactive_products',q=String(args.query||'').toLowerCase();return Promise.resolve(db.products.filter(p=>(p.is_active===false)===inactive&&[p.code,p.name,p.barcode].some(v=>String(v||'').toLowerCase().includes(q))));}
      case 'create_product': {const code=String(args.code||'').trim(),name=String(args.name||'').trim();if(!code||!name)return Promise.reject('الكود والاسم مطلوبان');if(db.products.some(p=>p.code===code))return Promise.reject('كود الصنف موجود بالفعل');let barcode=String(args.barcode||'').trim();if(!barcode)barcode='MAD'+String(Date.now()).slice(-10);if(db.products.some(p=>p.barcode===barcode))return Promise.reject('الباركود مستخدم بالفعل');const p={id:db.nextProductId++,code,name,unit:args.unit||'قطعة',barcode,purchase_price_minor:Number(args.purchasePriceMinor||0),sale_price_minor:Number(args.salePriceMinor||0),is_active:true};db.products.push(p);write(db);return Promise.resolve(p);}
      case 'update_product': {const p=db.products.find(x=>x.id===Number(args.id));if(!p)return Promise.reject('الصنف غير موجود');Object.assign(p,{code:String(args.code||p.code),name:String(args.name||p.name),unit:String(args.unit||p.unit),purchase_price_minor:Number(args.purchasePriceMinor||0),sale_price_minor:Number(args.salePriceMinor||0)});if(args.barcode&&args.barcode!==p.barcode){p.alternateBarcodes=[...(p.alternateBarcodes||[]),p.barcode];p.barcode=args.barcode;}write(db);return Promise.resolve(p);}
      case 'deactivate_product': case 'reactivate_product': {const p=db.products.find(x=>x.id===Number(args.id));if(!p)return Promise.reject('الصنف غير موجود');p.is_active=command==='reactivate_product';write(db);return Promise.resolve('تم تحديث حالة الصنف');}
      case 'find_product_by_barcode': {const b=String(args.barcode||'');return Promise.resolve(db.products.find(p=>p.barcode===b||(p.alternateBarcodes||[]).includes(b))||null);}
      case 'add_alternate_barcode': {const p=db.products.find(x=>x.id===Number(args.productId));if(!p)return Promise.reject('الصنف غير موجود');const b=String(args.barcode||'').trim();if(!b)return Promise.reject('الباركود مطلوب');p.alternateBarcodes=[...(p.alternateBarcodes||[]),b];write(db);return Promise.resolve({message:'تمت إضافة الباركود البديل'});}
      default: return Promise.reject(`الأمر «${command}» يحتاج قاعدة بيانات Tauri. وضع المتصفح للمعاينة فقط ويدعم تسجيل الدخول وإدارة الأصناف الأساسية.`);
    }
  }
  window.MadarAPI={isDesktop:!!nativeInvoke,invoke:(command,args)=>nativeInvoke?nativeInvoke(command,args):browserInvoke(command,args)};
  window.MadarDialogs=nativeDialog||{
    save: async()=>{throw new Error('اختيار الملفات والنسخ الاحتياطي يتطلب تطبيق سطح المكتب؛ وضع المتصفح للمعاينة فقط.');},
    open: async()=>{throw new Error('فتح الملفات يتطلب تطبيق سطح المكتب؛ وضع المتصفح للمعاينة فقط.');}
  };
  if(!nativeInvoke) document.documentElement.dataset.madarMode='browser';
})();
