# Pinned rlottie source

Origin: https://github.com/laggykiller/rlottie

Revision: `47b71a7e9072ac67b2e18c408d3668fed278fb93`

This is the exact renderer submodule pinned by rlottie-python v1.3.8
(`ac225342cab07d38a9fc1f92adaffa7d48f2309c`). Numera uses its native C API
directly. No Python wrapper is included in production artifacts.

The native library source subset is included to make builds independent of mutable
upstream branches, network clones, system rlottie installations, and bindgen.
The bundled image decoder is compiled into the library (`LOTTIE_MODULE=OFF`)
so external or embedded image assets do not require an optional shared plugin.

See COPYING and licenses/ for the MIT, Skia/BSD, FreeType, Pixman, stb,
RapidJSON, and Mozilla license notices associated with this source. These
notices must accompany redistributed binary artifacts.

Release CI compiles this source statically for Linux, macOS Intel/ARM, and
Windows. CMake and a C++ compiler are build dependencies only.

The subset retains the native public headers, C API, parser/renderer, SIMD,
image decoder, complete native dependency headers, CMake inputs, AUTHORS,
COPYING, and every license file. Upstream examples and players, sample art and
animations, screenshots/GIFs, tests, WebAssembly bindings, Meson/Visual Studio
project files, CI files, packaging recipes, and unrelated build scripts are
omitted.

Numera carries two build-only changes in CMakeLists.txt:

1. Do not unconditionally force the static MSVC CRT when building a static
   rlottie archive. The parent Cargo build selects the CRT to match Rust and
   the other native dependencies.
2. Configure only the native library, excluding the omitted upstream example,
   player, and test targets.

No parser, renderer, SIMD, or image-decoder source is changed.
