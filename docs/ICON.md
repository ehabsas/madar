# أيقونة مدار | Madar App Icon

المصدر المتجهي الأصلي: `src-tauri/icons/madar.svg`.

التصميم يستخدم صندوق مخزون داخل مدار/مسار دائري، بلون أزرق أساسي وتفاصيل سماوية. النصوص غير مضمنة في الأيقونة حتى تظل واضحة على Windows وmacOS وLinux وفي الأحجام الصغيرة.

## الملفات

- `icon.png`: مصدر PNG بدقة 1024×1024.
- `32x32.png`, `128x128.png`, `128x128@2x.png`: أيقونات سطح المكتب.
- `icon.ico`: أيقونة Windows متعددة المقاسات.
- `icon.icns`: حاوية أيقونات macOS.
- `Square*Logo.png` و`StoreLogo.png`: مقاسات مربعة إضافية لمنصات Windows.

إعدادات Tauri في `src-tauri/tauri.conf.json` تشير إلى الملفات التي يستخدمها bundler.

لتعديل التصميم، عدّل `madar.svg` ثم شغّل `scripts/generate-icons.sh` من جذر المشروع. هذا يتطلب Tauri CLI، وقد يغير ملفات الأيقونات المولدة؛ راجعها في Git بعد التوليد.
