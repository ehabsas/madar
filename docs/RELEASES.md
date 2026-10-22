# إصدارات مدار وحزم التوزيع

## ملفات الحزم

| الهدف | الحزمة | مكان البناء |
|---|---|---|
| Windows x64 | `.msi`, `*-setup.exe` (NSIS) | `bundle/msi/`, `bundle/nsis/` |
| macOS | `.app`, `.dmg` | `bundle/macos/`, `bundle/dmg/` |
| Ubuntu/Debian | `.deb`, `.AppImage` | `bundle/deb/`, `bundle/appimage/` |
| Fedora | `.rpm`, `.AppImage` | `bundle/rpm/`, `bundle/appimage/` |
| Arch Linux | `.pkg.tar.zst` | `bundle/arch/` |

هذه الأسماء تحدد المخرجات المتوقعة من ملفات البناء؛ يلزم إنشاء كل حزمة على النظام المناسب واختبارها. لم يتم إرفاق ملفات تنفيذية مسبقة البناء بهذه الشفرة.

## إصدار يدوي

1. حدّث رقم الإصدار في `src-tauri/tauri.conf.json` و`src-tauri/Cargo.toml` بشكل متوافق.
2. تأكد من نجاح `cargo check` واختبارات القبول على قاعدة اختبار.
3. شغّل ملف البناء الخاص بالنظام من `docs/BUILDING.md`.
4. اختبر الحزمة على تثبيت نظيف ولا ترفق قاعدة البيانات أو مفاتيح التوقيع أو الأسرار.
5. لتوزيع تجاري على Windows/macOS، جهّز توقيع الشيفرة وشهادات النظام المناسبة. لا تُضمّن مفاتيح التوقيع في Git.

## GitHub Actions

الملف `.github/workflows/build-desktop.yml` يعمل يدويًا من Actions أو عند دفع tag مثل `v0.1.0`. ينتج artifacts أصلية لـ Windows وmacOS وUbuntu. Fedora وArch لهما سكربتات بناء محلية لأنهما يحتاجان بيئة البناء الخاصة بهما.
