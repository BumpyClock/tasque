#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT" || {
	echo "Error: Failed to change to repository root: $REPO_ROOT" >&2
	exit 1
}
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
while IFS= read -r pkg; do
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
done < <(
	node - <<'NODE'
const { platformPackageManifests } = require('./npm/scripts/postinstall');
console.log('npm/package.json');
for (const { manifestPath } of platformPackageManifests('npm/platforms')) console.log(manifestPath);
NODE
)

# Copy binaries from artifacts/<rust-target>/ into platform package dirs
missing_binaries=()
while IFS=$'\t' read -r platform target bin_name tui_bin_name; do
	src_dir="$REPO_ROOT/artifacts/$target"
	dst_dir="$NPM_DIR/platforms/$platform"

	if [[ -f "$src_dir/$bin_name" ]]; then
		cp "$src_dir/$bin_name" "$dst_dir/$bin_name"
		chmod 755 "$dst_dir/$bin_name"
		echo "Copied $target/$bin_name -> platforms/$platform/"
	else
		missing_binaries+=("$platform:$src_dir/$bin_name")
	fi

	if [[ -f "$src_dir/$tui_bin_name" ]]; then
		cp "$src_dir/$tui_bin_name" "$dst_dir/$tui_bin_name"
		chmod 755 "$dst_dir/$tui_bin_name"
		echo "Copied $target/$tui_bin_name -> platforms/$platform/"
	else
		missing_binaries+=("$platform:$src_dir/$tui_bin_name")
	fi
done < <(
	node - <<'NODE'
const { platformPackageManifests } = require('./npm/scripts/postinstall');
for (const { platform, manifest } of platformPackageManifests('npm/platforms')) {
  const target = (manifest.tasque && manifest.tasque.target) || '';
  const files = Array.isArray(manifest.files) ? manifest.files : [];
  const bin = files.find((file) => file === 'tsq' || file === 'tsq.exe') || '';
  const tui = files.find((file) => file === 'tsq-tui' || file === 'tsq-tui.exe') || '';
  if (!target || !bin || !tui) {
    console.error(`Warning: skipping ${platform} - missing target/bin/tui in manifest`);
    continue;
  }
  console.log(`${platform}\t${target}\t${bin}\t${tui}`);
}
NODE
)

if ((${#missing_binaries[@]} > 0)); then
	echo "Error: missing expected platform binaries after copy step:" >&2
	for entry in "${missing_binaries[@]}"; do
		platform="${entry%%:*}"
		path="${entry#*:}"
		echo "  - $platform: $path" >&2
	done
	exit 1
fi

echo "Build complete. Ready to publish."
