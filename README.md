# مدار | Madar Business Management

برنامج سطح مكتب لإدارة المنتجات والمخزون والمشتريات والمبيعات، مبني باستخدام **Rust + Tauri 2 + SQLite**. الواجهة الحالية ثابتة داخل `src/`؛ لا يحتاج المشروع إلى Node.js/npm للبناء في وضعه الحالي. المشروع مرخّص بموجب **GNU General Public License الإصدار 3 فقط (GPL-3.0-only)**؛ راجع ملف [`LICENSE`](LICENSE) ودليل [`docs/LICENSING.md`](docs/LICENSING.md).

> **تنبيه الحالة:** المشروع ما زال قيد التطوير. لا تعتبره بديلًا إنتاجيًا عن SM Elite SQL قبل استكمال ترحيل قاعدة البيانات الحقيقية والتحقق من أرصدة المخزون والحسابات. اختبر كل ترقية على نسخة احتياطية منفصلة.

## الوظائف الحالية بالمشروع

- أصناف مع باركود أساسي وباركودات بديلة محفوظة كنص لدعم الأصفار البادئة.
- سجل حركات مخزون ومنع صرف كمية أكبر من المتاح.
- عملاء وموردون، وفواتير مشتريات ومبيعات بدورة مسودة/اعتماد، مع تحديث المخزون وسجل النشاط.
- مستخدمون محليون وكلمات مرور مجزأة وصلاحيات تفصيلية وسجل تدقيق.
- بيانات الشركة وإعدادات الضريبة، وعروض أسعار، ومعاينة المستندات التجارية قبل الطباعة، ومدفوعات وخزنة وتقارير.
- نسخ احتياطي واستعادة ونقاط أمان، وواجهة عربية/إنجليزية ووضع فاتح/داكن.
- تخصيص الحافظة لتجنب قائمة WebKitGTK الافتراضية على Linux؛ ما زال ينبغي اختبارها على كل بيئة سطح مكتب مستهدفة.

قد تتفاوت درجة اكتمال الوظائف وتغطية الاختبارات؛ راجع قوائم القبول والقيود في مجلد `docs/` قبل الاستخدام الفعلي.

## بدء التطوير

1. ثبّت متطلبات Tauri للنظام من [الدليل الرسمي](https://v2.tauri.app/start/prerequisites/) وRust stable.
2. ثبّت Tauri CLI مرة واحدة:

```bash
cargo install tauri-cli --locked
```

3. فحص Rust:

```bash
cd src-tauri
cargo check
```

4. تشغيل وضع التطوير من جذر المشروع:

```bash
cd ..
cargo tauri dev
```

## البناء والتوزيع

السكربتات الجاهزة موجودة في `scripts/`، ودليل البناء الكامل في [`docs/BUILDING.md`](docs/BUILDING.md).

| النظام | الأمر | المخرج المتوقع |
|---|---|---|
| Windows 10/11 | `./scripts/build-windows.ps1` من PowerShell | MSI + NSIS Setup EXE |
| macOS | `./scripts/build-macos.sh` | `.app` + `.dmg` |
| Ubuntu/Debian | `./scripts/build-ubuntu.sh` | `.deb` + `.AppImage` |
| Fedora | `./scripts/build-fedora.sh` | `.rpm` + `.AppImage` |
| Arch Linux | `./scripts/build-arch.sh` | `.pkg.tar.zst` |

تُحفظ النتائج تحت `src-tauri/target/release/bundle/`. لا يتم إنتاج جميع المنصات من عملية بناء محلية واحدة: يجب بناء Windows وmacOS على runners أو أجهزة مناسبة؛ خصوصًا MSI يحتاج Windows، وتوزيع macOS يحتاج التوقيع والتوثيق عند النشر العام. راجع [دليل Windows](docs/BUILDING.md#windows-1011-الطريقة-الموصى-بها-حاليًا).

يوجد GitHub Actions workflow في `.github/workflows/build-desktop.yml` لبناء Windows وmacOS وUbuntu يدويًا أو عند دفع tag يبدأ بـ `v`.

## هوية التطبيق

مصدر الأيقونة SVG: [`src-tauri/icons/madar.svg`](src-tauri/icons/madar.svg). ملفات PNG/ICO/ICNS المولدة وإعدادات bundler داخل `src-tauri/icons/` و`src-tauri/tauri.conf.json`. راجع [`docs/ICON.md`](docs/ICON.md).

## البيانات والأمان

- قاعدة البيانات محلية؛ لا ترفع ملف DB أو النسخ الاحتياطية إلى مستودع Git.
- قبل الاستعادة أو ترحيل مخطط قاعدة البيانات، أنشئ نسخة أمان واختبر على قاعدة منفصلة.
- سجل التدقيق حماية على مستوى التطبيق المحلي، وليس دفترًا غير قابل للتلاعب أمام مستخدم يملك وصولًا مباشرًا لملف SQLite أو نظام التشغيل.
- لا توجد مزامنة سحابية مفعّلة.

## الوثائق

- [`LICENSE`](LICENSE) و[`docs/LICENSING.md`](docs/LICENSING.md) — رخصة GPL-3.0-only ونطاقها وملاحظات التبعيات الخارجية.
- [`docs/BUILDING.md`](docs/BUILDING.md) — متطلبات وخطوات البناء لكل نظام.
- [`docs/RELEASES.md`](docs/RELEASES.md) — الحزم وسير إصدار النسخ.
- [`docs/ICON.md`](docs/ICON.md) — ملف الأيقونة وتوليد الأحجام.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — حالة المراحل والخطوات المتبقية.
- [`docs/BARCODE-MIGRATION.md`](docs/BARCODE-MIGRATION.md) — قواعد الباركود وترحيله.
- [`docs/PHASES-1-3-ACCEPTANCE.md`](docs/PHASES-1-3-ACCEPTANCE.md) — اختبارات قبول العملاء والموردين والمشتريات والمبيعات.
- [`docs/PRINTING.md`](docs/PRINTING.md) — سلوك معاينة الطباعة واختبارات المستندات.


## Frontend structure and browser preview

- `src/index.html`: page structure and asset references.
- `src/styles.css`: shared responsive styling for phone, tablet and desktop.
- `src/app.js`: UI behavior.
- `src/browser-api.js`: selects native Tauri commands on desktop or a clearly labeled localStorage demo adapter in a normal browser.
- `src-tauri/src/lib.rs`: shared backend setup, database helpers, app state and command registration.
- `src-tauri/src/*_commands.rs`: command handlers grouped by domain (authentication, products, inventory, parties, finance and backups).

### Browser preview

Open `src/index.html` in a browser or serve the `src` directory with a local static server. The browser adapter supports demo authentication and basic product management in localStorage. It is **not** a production replacement for the SQLite/Tauri backend: financial documents, stock movements, user administration, file dialogs and backup/restore still require the desktop app. Do not use browser demo data as business records. Browser data can be cleared by clearing this site's storage.

For local serving, from the project root run `python -m http.server 4173 --directory src`, then open `http://localhost:4173`.

## Lazy-loaded page architecture

The desktop UI uses a persistent shell in `src/index.html`, shared runtime code in `src/app.js`, and a page loader in `src/pages-loader.js`. Only the dashboard template is requested during startup. Other page templates are fetched when selected and their DOM nodes are cached to preserve form state when navigating away and back.

Each page has a matching set of files under `src/pages/`: `<page>.html`, `<page>.js`, and `<page>.css`. Page JavaScript modules are loaded once on first visit and expose a `mount()` hook through `window.MadarPageModules`; page-specific CSS is linked on demand and remains scoped to its page root. Shared styling stays in `src/styles.css` to avoid duplicating common components. `onElement()` queues listeners for controls belonging to templates that have not been loaded yet and binds them when the relevant page enters the DOM.

This organization keeps Tauri commands and shared application services in the shared runtime while moving template delivery and page initialization to an on-demand lifecycle.


## إضافات تشغيلية حديثة

- لكل صنف حد أدنى اختياري للمخزون، وتظهر قائمة بالأصناف النشطة التي وصل رصيدها إلى الحد أو أقل في شاشة المخزون.
- يمكن فتح طباعة ملصق بيانات الصنف من قائمة المنتجات، مع الاسم والكود والباركود النصي والسعر. إخراج باركود Code 128 مرئي قابل للمسح واختباره على طابعات فعلية ما زال خطوة لاحقة؛ لا تعتبر الملصقات الحالية بديلًا عن طباعة باركود مرئي.
- سجل النشاط يحمّل السجلات على دفعات مع تنفيذ البحث في قاعدة البيانات قبل تحديد الدفعة.
- معاينة الفواتير وعروض الأسعار متاحة قبل فتح نافذة الطباعة.

راجع [خطة التنفيذ](docs/ROADMAP.md) لمعرفة الوظائف المكتملة والمتبقية.
