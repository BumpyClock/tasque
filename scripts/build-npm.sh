#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NPM_DIR="$REPO_ROOT/npm"

SKILLS_SRC="$REPO_ROOT/SKILLS"
if [[ -d "$SKILLS_SRC" ]]; then
  rm -rf "$NPM_DIR/SKILLS"
  cp -R "$SKILLS_SRC" "$NPM_DIR/SKILLS"

  for platform_dir in "$NPM_DIR/platforms"/*; do
    if [[ -d "$platform_dir" ]]; then
      rm -rf "$platform_dir/SKILLS"
      cp -R "$SKILLS_SRC" "$platform_dir/SKILLS"
    fi
  done

  if [[ ! -f "$NPM_DIR/SKILLS/tasque/SKILL.md" ]]; then
    echo "Error: missing npm SKILLS/tasque/SKILL.md after copy" >&2
    exit 1
  fi

  for platform_dir in "$NPM_DIR/platforms"/*; do
    if [[ -d "$platform_dir" && ! -f "$platform_dir/SKILLS/tasque/SKILL.md" ]]; then
      echo "Error: missing SKILLS/tasque/SKILL.md in $platform_dir" >&2
      exit 1
    fi
  done
else
  echo "Warning: SKILLS directory not found at $SKILLS_SRC" >&2
fi

# Read version from Cargo.toml
VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Version: $VERSION"

# Patch all package.json files with the correct version
for pkg in \
  "$NPM_DIR/package.json" \
  "$NPM_DIR/platforms/darwin-arm64/package.json" \
  "$NPM_DIR/platforms/darwin-x64/package.json" \
  "$NPM_DIR/platforms/linux-x64-gnu/package.json" \
  "$NPM_DIR/platforms/linux-arm64-gnu/package.json" \
  "$NPM_DIR/platforms/win32-x64-msvc/package.json" \
  "$NPM_DIR/platforms/win32-arm64-msvc/package.json"; do
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$pkg', 'utf8'));
    pkg.version = '$VERSION';
    // Also update optionalDependencies versions if present
    if (pkg.optionalDependencies) {
      for (const dep of Object.keys(pkg.optionalDependencies)) {
        pkg.optionalDependencies[dep] = '$VERSION';
      }
    }
    fs.writeFileSync('$pkg', JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "Patched $pkg -> $VERSION"
done

# Platform mapping: directory -> rust target -> binary name
declare -A TARGETS=(
  ["darwin-arm64"]="aarch64-apple-darwin"
  ["darwin-x64"]="x86_64-apple-darwin"
  ["linux-x64-gnu"]="x86_64-unknown-linux-gnu"
  ["linux-arm64-gnu"]="aarch64-unknown-linux-gnu"
  ["win32-x64-msvc"]="x86_64-pc-windows-msvc"
  ["win32-arm64-msvc"]="aarch64-pc-windows-msvc"
)

# Copy binaries from artifacts/<rust-target>/ into platform package dirs
missing_binaries=()
for platform in "${!TARGETS[@]}"; do
  target="${TARGETS[$platform]}"
  src_dir="$REPO_ROOT/artifacts/$target"
  dst_dir="$NPM_DIR/platforms/$platform"

  if [[ "$platform" == win32-* ]]; then
    bin_name="tsq.exe"
  else
    bin_name="tsq"
  fi

  if [[ -f "$src_dir/$bin_name" ]]; then
    cp "$src_dir/$bin_name" "$dst_dir/$bin_name"
    chmod 755 "$dst_dir/$bin_name"
    echo "Copied $target/$bin_name -> platforms/$platform/"
  else
    missing_binaries+=("$platform:$src_dir/$bin_name")
  fi
done

if (( ${#missing_binaries[@]} > 0 )); then
  echo "Error: missing expected platform binaries after copy step:" >&2
  for entry in "${missing_binaries[@]}"; do
    platform="${entry%%:*}"
    path="${entry#*:}"
    echo "  - $platform: $path" >&2
  done
  exit 1
fi

echo "Build complete. Ready to publish."
