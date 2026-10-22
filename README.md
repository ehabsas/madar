# مدار | Madar Business Management

برنامج سطح مكتب لإدارة المنتجات والمخزون والمشتريات والمبيعات، مبني باستخدام **Rust + Tauri 2 + SQLite**. الواجهة الحالية ثابتة داخل `src/`؛ لا يحتاج المشروع إلى Node.js/npm للبناء في وضعه الحالي.

> **تنبيه الحالة:** المشروع ما زال قيد التطوير. لا تعتبره بديلًا إنتاجيًا عن SM Elite SQL قبل استكمال ترحيل قاعدة البيانات الحقيقية والتحقق من أرصدة المخزون والحسابات. اختبر كل ترقية على نسخة احتياطية منفصلة.

## الوظائف الحالية بالمشروع

- أصناف مع باركود أساسي وباركودات بديلة محفوظة كنص لدعم الأصفار البادئة.
- سجل حركات مخزون ومنع صرف كمية أكبر من المتاح.
- عملاء وموردون، وفواتير مشتريات ومبيعات بدورة مسودة/اعتماد، مع تحديث المخزون وسجل النشاط.
- مستخدمون محليون وكلمات مرور مجزأة وصلاحيات تفصيلية وسجل تدقيق.
- بيانات الشركة وإعدادات الضريبة، وعروض أسعار وطباعة مستندات، ومدفوعات وخزنة وتقارير.
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

- [`docs/BUILDING.md`](docs/BUILDING.md) — متطلبات وخطوات البناء لكل نظام.
- [`docs/RELEASES.md`](docs/RELEASES.md) — الحزم وسير إصدار النسخ.
- [`docs/ICON.md`](docs/ICON.md) — ملف الأيقونة وتوليد الأحجام.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — حالة المراحل والخطوات المتبقية.
- [`docs/BARCODE-MIGRATION.md`](docs/BARCODE-MIGRATION.md) — قواعد الباركود وترحيله.
- [`docs/PHASES-1-3-ACCEPTANCE.md`](docs/PHASES-1-3-ACCEPTANCE.md) — اختبارات قبول العملاء والموردين والمشتريات والمبيعات.
