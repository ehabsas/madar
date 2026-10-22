# بناء وتوزيع مدار | Build Madar

هذا الدليل يوضح بناء نسخة تطويرية وحزم التوزيع من نفس مصدر المشروع. الواجهة الحالية ملفات ثابتة داخل `src/`، لذلك لا يحتاج البناء إلى Node.js أو npm. يلزم Rust وTauri CLI واعتماديات النظام الأصلي.

## ملفات البناء المضافة

| النظام | ملف البناء | المخرجات |
|---|---|---|
| Windows 10/11 x64 | `scripts/build-windows.ps1` | مثبتا `.msi` و`-setup.exe` عبر NSIS |
| macOS | `scripts/build-macos.sh` | `.app` و`.dmg` |
| Ubuntu/Debian | `scripts/build-ubuntu.sh` | `.deb` و`.AppImage` |
| Fedora | `scripts/build-fedora.sh` | `.rpm` و`.AppImage` |
| Arch Linux | `scripts/build-arch.sh` | حزمة pacman بصيغة `.pkg.tar.zst` |
| CI | `.github/workflows/build-desktop.yml` | بناء Windows وmacOS وUbuntu على runners أصلية عند تشغيل workflow أو دفع tag يبدأ بـ `v` |

تظهر الملفات عادةً داخل `src-tauri/target/release/bundle/`. حزمة Arch المحلية داخل `src-tauri/target/release/bundle/arch/`.

## المتطلبات العامة

- Rust stable وCargo.
- تثبيت Tauri CLI مرة واحدة:

```bash
cargo install tauri-cli --locked
```

- افتح الطرفية من جذر المشروع، وهو المجلد الذي يحتوي `README.md` و`src-tauri/`.

## Windows 10/11 (الطريقة الموصى بها حاليًا)

**الأفضل أن يتم بناء مثبت Windows على Windows نفسه.** بناء MSI على Linux غير مدعوم؛ أما بناء NSIS من Linux فهو تجريبي وله متطلبات إضافية. راجع الدليل الرسمي: https://tauri.app/distribute/windows-installer/ .

### 1. تثبيت الأدوات

1. ثبّت Microsoft C++ Build Tools من https://visualstudio.microsoft.com/visual-cpp-build-tools/ .
2. في Visual Studio Installer اختر workload **Desktop development with C++**، وتأكد من وجود MSVC x64/x86 build tools وWindows 10/11 SDK.
3. ثبّت Rust باستخدام https://rustup.rs/ أو:

```powershell
winget install --id Rustlang.Rustup -e
```

4. WebView2 Runtime موجود عادةً على Windows 10/11. إذا كان مفقودًا، ثبّت Evergreen Runtime من: https://developer.microsoft.com/microsoft-edge/webview2/ .
5. افتح PowerShell جديدًا وتأكد:

```powershell
rustc --version
cargo --version
```

6. من جذر Madar ثبّت Tauri CLI:

```powershell
cargo install tauri-cli --locked
cargo tauri --version
```

### 2. بناء المثبتين

شغّل PowerShell داخل جذر المشروع، ثم:

```powershell
.\scripts\build-windows.ps1
```

أو نفّذ الأمر مباشرةً:

```powershell
cargo tauri build --bundles msi,nsis
```

المخرجات:

- MSI: `src-tauri\target\release\bundle\msi\`
- NSIS setup EXE: `src-tauri\target\release\bundle\nsis\`

إذا فشل MSI برسالة حول `light.exe`، افتح **Settings → Apps → Optional features → More Windows features** وتأكد من تفعيل **VBSCRIPT**. تحذير SmartScreen متوقع للمثبتات غير الموقعة؛ التوزيع على نطاق واسع يحتاج شهادة Authenticode وتوقيعًا مناسبًا.

### هل يمكن بناؤه على Fedora دون Windows؟

يمكن تجربة cross-compilation لمثبت NSIS فقط، لكن Tauri يذكر أن هذا المسار أقل اختبارًا وتجريبي، ولا ينشئ MSI. لذلك المسار الموصى به للمستخدم هو Windows مباشرة أو workflow في GitHub Actions. يلزم رفع المشروع إلى مستودع GitHub ثم فتح تبويب Actions وتشغيل **Build desktop installers**، وبعد نجاح التشغيل نزّل artifact `madar-windows-x64`. لا ترفع قاعدة بيانات حقيقية أو ملفات أسرار إلى المستودع.

## macOS

لا يمكن إنتاج تطبيق macOS قابل للتوزيع بشكل موثوق من Fedora. استخدم Mac مع Xcode Command Line Tools وRust:

```bash
xcode-select --install
cargo install tauri-cli --locked
./scripts/build-macos.sh
```

تظهر المخرجات في `src-tauri/target/release/bundle/`. التوزيع خارج App Store يتطلب عادةً التوقيع والتوثيق (notarization) من Apple. ويمكن استخدام GitHub Actions على `macos-latest` للحصول على artifact أولي غير موقّع.

## Ubuntu / Debian

اعتماديات البناء بحسب متطلبات Tauri 2 الرسمية:

```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libfuse2
```

ثم:

```bash
./scripts/build-ubuntu.sh
```

المخرجات: `deb/` و`appimage/` ضمن `src-tauri/target/release/bundle/`. يتم بناء DEB على Ubuntu 22.04 في CI كخط أساس توافق مع الأنظمة الأقدم؛ بناء Linux على توزيعة أحدث قد يطلب إصدار glibc أحدث عند التشغيل. المصدر: https://v2.tauri.app/distribute/debian/ .

## Fedora

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel \
  gcc gcc-c++ make patchelf rpm-build
cargo install tauri-cli --locked
./scripts/build-fedora.sh
```

ينشئ Tauri حزمة RPM وAppImage في مجلد `bundle/`.

## Arch Linux

```bash
sudo pacman -Syu
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file \
  openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool rustup
rustup default stable
cargo install tauri-cli --locked
./scripts/build-arch.sh
```

شغّل `scripts/build-arch.sh` كمستخدم عادي **بدون sudo**؛ يبني البرنامج ثم يجهّز سياقًا محليًا لـ `makepkg` وينشئ حزمة pacman. الملف `packaging/arch/PKGBUILD.template` هو قالب مرجعي. نشر الحزمة في AUR يتطلب مستودعًا عامًا وبيانات نشر وهاشات مصادر صحيحة؛ هذه ليست معدّة تلقائيًا.

## إعادة توليد الأيقونات

مصدر الرسم المتجهي:

`src-tauri/icons/madar.svg`

الأيقونات التي يستخدمها Tauri موجودة مسبقًا في `src-tauri/icons/`. إذا عدّلت SVG، شغّل:

```bash
./scripts/generate-icons.sh
```

يستخدم الأمر `cargo tauri icon` لتوليد مقاسات وأشكال الأيقونات المطلوبة للمنصات، وفق دليل Tauri: https://tauri.app/reference/cli/ .

## الاختبار قبل مشاركة الإصدار

1. `cargo check` من `src-tauri`.
2. تجربة `cargo tauri dev` قبل بناء release.
3. اختبار تثبيت الحزمة على جهاز/VM نظيف لكل نظام مستهدف.
4. تجربة تسجيل الدخول والصلاحيات، نسخ قاعدة البيانات واستعادتها، إضافة صنف وباركود، دورة شراء/بيع على قاعدة اختبار، الطباعة، وتغيير اللغة والوضع.
5. لا توزّع قاعدة SQLite الشخصية أو النسخ الاحتياطية ضمن مثبت البرنامج. خذ نسخًا احتياطية منفصلة قبل اختبار الترحيل.
6. ملفات البناء تهيّئ عملية الإنشاء؛ لا تعني أن كل حزم الأنظمة قد بُنيت واختُبرت بالفعل في بيئة المؤلف.

## مراجع Tauri الرسمية

- المتطلبات: https://v2.tauri.app/start/prerequisites/
- التوزيع والبناء: https://v2.tauri.app/distribute/
- مثبّت Windows: https://tauri.app/distribute/windows-installer/
- AppImage: https://tauri.app/distribute/appimage/
- RPM: https://v2.tauri.app/distribute/rpm/
- AUR: https://v2.tauri.app/distribute/aur/


## Browser preview (not production mode)

From the repository root run `python -m http.server 4173 --directory src` and open `http://localhost:4173`. The static browser preview stores demo users and products in that browser's localStorage. It does not access the desktop SQLite database and does not support production invoices, inventory movements, file dialogs or backups. Use Tauri for real business records.
