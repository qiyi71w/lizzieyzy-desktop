#!/usr/bin/env bash
# Builds a self-contained macOS KataGo package from an official release tag.
#
#   KATAGO_TAG=v1.18.2 tools/katago-macos/build.sh <arch: arm64|x86_64> <KataGo checkout at the tag> <output dir>
#
# The upstream GitHub releases ship no macOS binaries. This produces the same flat layout as the
# official Windows/Linux zips (katago, default_gtp.cfg, analysis_example.cfg) plus lib/ with every
# non-system dylib rewritten to @executable_path/@loader_path, licenses/ and BUILD-INFO.json.
# Dependencies come from Homebrew bottles installed on the build host; nothing outside the system
# libraries is referenced by the result.
set -euo pipefail

ARCH="$1"
SOURCE="$(cd "$2" && pwd)"
mkdir -p "$3"
OUTPUT="$(cd "$3" && pwd)"
TAG="${KATAGO_TAG:?KATAGO_TAG is required}"
MIN_MACOS="15.0"

case "$ARCH" in
  arm64) LABEL="arm64" ;;
  x86_64) LABEL="x64" ;;
  *) echo "unsupported arch: $ARCH" >&2; exit 2 ;;
esac
[[ "$(uname -m)" == "$ARCH" ]] || { echo "host is $(uname -m), not $ARCH" >&2; exit 2; }

COMMIT="$(git -C "$SOURCE" rev-parse HEAD)"
TAG_COMMIT="$(git -C "$SOURCE" rev-parse "refs/tags/$TAG^{commit}")"
[[ "$COMMIT" == "$TAG_COMMIT" ]] || { echo "checkout $COMMIT is not tag $TAG ($TAG_COMMIT)" >&2; exit 2; }
VERSION="${TAG#v}"
NAME="katago-$TAG-metal-macos-$LABEL"
BREW="$(brew --prefix)"
CELLAR="$(realpath "$(brew --cellar)")"
BUILD="$OUTPUT/build-$ARCH"
PKG="$OUTPUT/$NAME"
rm -rf "$BUILD" "$PKG" "$OUTPUT/$NAME.zip"
mkdir -p "$BUILD" "$PKG/lib" "$PKG/licenses/KataGo"

export PKG_CONFIG_PATH="$(brew --prefix protobuf)/lib/pkgconfig:$(brew --prefix abseil)/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
LIBZIP="$(brew --prefix libzip)"
cmake -S "$SOURCE/cpp" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
  -DUSE_BACKEND=METAL \
  -DBUILD_DISTRIBUTED=1 \
  -DOPENSSL_ROOT_DIR="$(brew --prefix openssl@3)" \
  -DCMAKE_PREFIX_PATH="$BREW" \
  -DLIBZIP_INCLUDE_DIR_ZIP="$LIBZIP/include" \
  -DLIBZIP_INCLUDE_DIR_ZIPCONF="$LIBZIP/include" \
  -DLIBZIP_LIBRARY="$LIBZIP/lib/libzip.dylib" \
  -DCMAKE_Swift_FLAGS="-Xlinker -headerpad_max_install_names" \
  -DCMAKE_EXE_LINKER_FLAGS="-Wl,-headerpad_max_install_names"
cmake --build "$BUILD" --target katago --parallel "$(sysctl -n hw.logicalcpu)"

cp "$BUILD/katago" "$PKG/katago"
cp "$SOURCE/cpp/configs/gtp_example.cfg" "$PKG/default_gtp.cfg"
cp "$SOURCE/cpp/configs/analysis_example.cfg" "$PKG/analysis_example.cfg"
cp "$SOURCE/LICENSE" "$PKG/licenses/KataGo/LICENSE"

# KataGo's LICENSE does not cover the components vendored under cpp/external nor cpp/core/sha2.cpp.
# Ship every notice they carry; components whose license lives only in a source header are
# extracted from that header. A component with neither stops the build.
copy_header() { # <source file> <destination>: the first /* ... */ comment block
  awk 'index($0, "/*") {inside = 1} inside {print} inside && index($0, "*/") {exit}' "$1" > "$2"
  grep -q "Copyright" "$2" || { echo "no license header in $1" >&2; exit 1; }
}
mkdir -p "$PKG/licenses/KataGo/external"
copy_header "$SOURCE/cpp/core/sha2.cpp" "$PKG/licenses/KataGo/sha2.cpp.txt"
for component in "$SOURCE"/cpp/external/*/; do
  name="$(basename "$component")"
  found=0
  while IFS= read -r -d '' file; do
    dest="$PKG/licenses/KataGo/external/$name/${file#"$component"}"
    mkdir -p "$(dirname "$dest")"
    cp "$file" "$dest"
    found=1
  done < <(find "$component" -type f \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -iname 'NOTICE*' \) -print0)
  if ((found == 0)); then
    case "$name" in
      nlohmann_json)
        mkdir -p "$PKG/licenses/KataGo/external/$name"
        copy_header "$component/json.hpp" "$PKG/licenses/KataGo/external/$name/LICENSE.txt"
        ;;
      *) echo "no license notice for vendored component $name" >&2; exit 1 ;;
    esac
  fi
done

# Non-system install names referenced by a Mach-O file (skips its own id line for dylibs).
deps() {
  local skip=1
  [[ "$1" == *.dylib ]] && skip=2
  otool -L "$1" | tail -n "+$skip" | sed -n '2,$p' | awk '{print $1}' \
    | grep -Ev '^(/usr/lib/|/System/|@rpath/libswift|@executable_path/|@loader_path/)' || true
}
# Resolve a reference (absolute or @rpath into the Homebrew prefix) to a real file.
resolve() {
  local ref="$1" name dir
  if [[ "$ref" == @rpath/* ]]; then
    name="${ref#@rpath/}"
    for dir in "$BREW/lib" "$BREW"/opt/*/lib; do
      if [[ -e "$dir/$name" ]]; then realpath "$dir/$name"; return; fi
    done
    echo "unresolved $ref" >&2
    return 1
  fi
  realpath "$ref"
}

declare -A BUNDLED=()
queue=("$PKG/katago")
while ((${#queue[@]})); do
  file="${queue[0]}"
  queue=("${queue[@]:1}")
  while read -r ref; do
    [[ -n "$ref" ]] || continue
    real="$(resolve "$ref")"
    base="$(basename "$ref")"
    if [[ -z "${BUNDLED[$base]:-}" ]]; then
      BUNDLED[$base]="$real"
      cp -L "$real" "$PKG/lib/$base"
      chmod u+w "$PKG/lib/$base"
      install_name_tool -id "@loader_path/$base" "$PKG/lib/$base"
      queue+=("$PKG/lib/$base")
    fi
    if [[ "$file" == "$PKG/katago" ]]; then
      install_name_tool -change "$ref" "@executable_path/lib/$base" "$file"
    else
      install_name_tool -change "$ref" "@loader_path/$base" "$file"
    fi
  done < <(deps "$file")
done

# Licenses of every bundled library, from the Homebrew keg that provided it.
FORMULAE=()
for base in "${!BUNDLED[@]}"; do
  real="${BUNDLED[$base]}"
  rel="${real#"$CELLAR"/}"
  [[ "$rel" != "$real" ]] || { echo "$real is not in the Homebrew Cellar" >&2; exit 1; }
  formula="${rel%%/*}"
  rest="${rel#*/}"
  version="${rest%%/*}"
  keg="$CELLAR/$formula/$version"
  FORMULAE+=("$formula $version")
  dest="$PKG/licenses/$formula"
  [[ -d "$dest" ]] && continue
  mkdir -p "$dest"
  found=0
  for f in "$keg"/LICENSE* "$keg"/COPYING* "$keg"/NOTICE* "$keg"/share/doc/*/LICENSE* "$keg"/share/doc/*/COPYING*; do
    if [[ -f "$f" ]]; then cp "$f" "$dest/"; found=1; fi
  done
  ((found)) || { echo "no license file in $keg" >&2; exit 1; }
done

for f in "$PKG"/lib/*.dylib "$PKG/katago"; do
  codesign --force --sign - "$f"
done

# Audit: no Homebrew or absolute non-system references remain; minimum OS within support.
for f in "$PKG/katago" "$PKG"/lib/*.dylib; do
  if otool -L "$f" | sed -n '2,$p' | awk '{print $1}' \
      | grep -Ev '^(/usr/lib/|/System/|@rpath/libswift|@executable_path/lib/|@loader_path/)'; then
    echo "unbundled reference in $f" >&2
    exit 1
  fi
  minos="$(otool -l "$f" | awk '/LC_BUILD_VERSION/{b=1} b&&/minos/{print $2; exit}')"
  [[ -n "$minos" ]] || { echo "no minos in $f" >&2; exit 1; }
  if ! python3 -c 'import sys; v=lambda s: tuple(map(int, s.split("."))); sys.exit(v(sys.argv[1]) > v(sys.argv[2]))' "$minos" "$MIN_MACOS"; then
    echo "$f requires macOS $minos > $MIN_MACOS" >&2
    exit 1
  fi
done

"$PKG/katago" version | tee "$OUTPUT/version-$ARCH.txt"
grep -q "KataGo v$VERSION" "$OUTPUT/version-$ARCH.txt"
grep -q "contributing to online distributed" "$OUTPUT/version-$ARCH.txt"
"$PKG/katago" runtests > "$OUTPUT/runtests-$ARCH.txt" 2>&1
tail -n 3 "$OUTPUT/runtests-$ARCH.txt"
grep -q "All tests passed" "$OUTPUT/runtests-$ARCH.txt"

deps_json="$(printf '%s\n' "${FORMULAE[@]}" | sort -u \
  | awk 'BEGIN{ORS=""; print "["} {if(NR>1) print ","; printf "{\"formula\":\"%s\",\"version\":\"%s\"}", $1, $2} END{print "]"}')"
cat > "$PKG/BUILD-INFO.json" <<EOF
{
  "source": "https://github.com/lightvector/KataGo",
  "tag": "$TAG",
  "commit": "$COMMIT",
  "version": "$VERSION",
  "backend": "METAL",
  "buildDistributed": true,
  "architecture": "$ARCH",
  "minimumMacOS": "$MIN_MACOS",
  "builtOnMacOS": "$(sw_vers -productVersion)",
  "xcode": "$(xcodebuild -version | head -1)",
  "bundledHomebrewFormulae": $deps_json,
  "workflowRun": "${GITHUB_SERVER_URL:-}/${GITHUB_REPOSITORY:-}/actions/runs/${GITHUB_RUN_ID:-}"
}
EOF

(cd "$PKG" && zip -X -r -q "$OUTPUT/$NAME.zip" .)
shasum -a 256 "$OUTPUT/$NAME.zip" | tee "$OUTPUT/$NAME.zip.sha256"
