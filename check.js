
const invoke = window.__TAURI__?.core?.invoke;
const statusEl=document.getElementById('status');
const money=n=>(Number(n||0)/100).toFixed(2);
async function refreshProducts(){if(!invoke){statusEl.textContent='واجهة المعاينة فقط: افتح المشروع داخل تطبيق Tauri لتفعيل حفظ البيانات.';return;}try{const rows=await invoke('list_products');document.getElementById('products').innerHTML=rows.map(p=>`<tr><td>${escapeHtml(p.code)}</td><td>${escapeHtml(p.name)}</td><td>${escapeHtml(p.unit)}</td><td><code>${escapeHtml(p.barcode)}</code></td><td>${money(p.sale_price_minor)}</td></tr>`).join('')||'<tr><td colspan="5">لا توجد أصناف حتى الآن.</td></tr>';}catch(e){statusEl.textContent='تعذر تحميل الأصناف: '+e;}}
function escapeHtml(v){return String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
document.getElementById('productForm').addEventListener('submit',async e=>{e.preventDefault();if(!invoke){statusEl.textContent='الحفظ متاح عند تشغيل التطبيق داخل Tauri بعد إعداد بيئة البناء.';return;}try{const p=await invoke('create_product',{code:document.getElementById('code').value,name:document.getElementById('name').value,unit:document.getElementById('unit').value,barcode:document.getElementById('barcode').value||null,purchasePriceMinor:Math.round(Number(document.getElementById('purchase').value)*100),salePriceMinor:Math.round(Number(document.getElementById('sale').value)*100)});statusEl.textContent=`تم حفظ الصنف: ${p.name}
الباركود: ${p.barcode}`;document.getElementById('barcode').value='';await refreshProducts();}catch(err){statusEl.textContent='فشل الحفظ: '+err;}});
document.getElementById('refresh').addEventListener('click',refreshProducts);
const backupStatus=document.getElementById('backupStatus');
const restorePointsEl=document.getElementById('restorePoints');
const dialog=window.__TAURI__?.dialog;
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
  restorePointsEl.innerHTML=points.map((p,i)=>`<div class="restore-item"><div class="restore-meta"><strong>${i===0?'أحدث نقطة استعادة':'نقطة استعادة '+(i+1)}</strong><span class="muted">${new Date(Number(p.modified_at_secs)*1000).toLocaleString('ar-EG')}</span><span class="muted">حجم النسخة: ${(Number(p.size_bytes)/1024/1024).toFixed(2)} ميجابايت</span><code>${escapeHtml(p.file_name)}</code></div><button class="btn danger restore-point-btn" data-path="${escapeHtml(p.path)}" type="button">استعادة هذه النسخة</button></div>`).join('');
  restorePointsEl.querySelectorAll('.restore-point-btn').forEach(btn=>btn.addEventListener('click',async()=>{
   const path=btn.dataset.path;
   if(!(await confirmRestore('سيتم استبدال البيانات الحالية بمحتويات نقطة الاستعادة المحددة. بعد الضغط على موافق فقط، سيحفظ مدار الوضع الحالي كنقطة أمان ثم يبدأ الاستعادة. هل تريد المتابعة؟'))) {backupStatus.textContent='تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.';return;}
   btn.disabled=true;backupStatus.textContent='جارٍ التحقق من نقطة الاستعادة وتطبيقها...';
   try{const result=await invoke('restore_backup',{backupPath:path});backupStatus.textContent=result+'\nتم تحديث البيانات.';await refreshProducts();await refreshRestorePoints();}
   catch(err){backupStatus.textContent='فشلت الاستعادة: '+err;btn.disabled=false;}
  }));
 }catch(err){restorePointsEl.innerHTML='<div class="restore-empty muted">تعذر تحميل نقاط الاستعادة: '+escapeHtml(err)+'</div>';}
}
document.getElementById('refreshRestorePoints').addEventListener('click',refreshRestorePoints);
function timestampForFile(){const d=new Date();const pad=n=>String(n).padStart(2,'0');return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}_${pad(d.getHours())}-${pad(d.getMinutes())}-${pad(d.getSeconds())}`;}
document.getElementById('backupBtn').addEventListener('click',async()=>{
 if(!invoke||!dialog?.save){backupStatus.textContent='ميزة اختيار الملفات متاحة عند تشغيل نسخة Tauri المبنية من المشروع.';return;}
 try{const destination=await dialog.save({title:'حفظ نسخة احتياطية من مدار',defaultPath:`Madar-Backup-${timestampForFile()}.db`,filters:[{name:'قاعدة بيانات SQLite',extensions:['db','sqlite']}]});if(!destination){backupStatus.textContent='تم إلغاء إنشاء النسخة الاحتياطية.';return;}backupStatus.textContent='جارٍ إنشاء النسخة الاحتياطية والتحقق منها...';const saved=await invoke('create_backup',{destination});backupStatus.textContent=`تم إنشاء النسخة الاحتياطية والتحقق من سلامتها:
${saved}`;}catch(err){backupStatus.textContent='فشل إنشاء النسخة الاحتياطية: '+err;}
});
document.getElementById('restoreBtn').addEventListener('click',async()=>{
 if(!invoke||!dialog?.open){backupStatus.textContent='ميزة اختيار الملفات متاحة عند تشغيل نسخة Tauri المبنية من المشروع.';return;}
 try{const selected=await dialog.open({title:'اختيار نسخة مدار للاستعادة',multiple:false,directory:false,filters:[{name:'قاعدة بيانات SQLite',extensions:['db','sqlite']}]});if(!selected){backupStatus.textContent='تم إلغاء الاستعادة.';return;}if(!(await confirmRestore('تحذير: ستُستبدل البيانات الحالية بمحتويات النسخة المختارة. بعد الضغط على موافق فقط، سيحتفظ مدار بنسخة أمان من قاعدة البيانات الحالية ثم يبدأ الاستعادة. هل تريد المتابعة؟'))){backupStatus.textContent='تم إلغاء الاستعادة. لم يتم إنشاء نسخة أمان.';return;}backupStatus.textContent='جارٍ التحقق من النسخة واستعادة البيانات...';const result=await invoke('restore_backup',{backupPath:selected});backupStatus.textContent=result+'\nسيُعاد تحميل قائمة الأصناف الآن.';await refreshProducts();await refreshRestorePoints();}catch(err){backupStatus.textContent='فشلت الاستعادة: '+err;}
});
refreshProducts();
refreshRestorePoints();
