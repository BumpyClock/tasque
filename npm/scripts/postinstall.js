"use strict";

const https = require("https");
const { createGunzip } = require("zlib");
const fs = require("fs");
const path = require("path");

const PLATFORMS = {
  "darwin arm64": { pkg: "@bumpyclock/tasque-darwin-arm64", bin: "tsq" },
  "darwin x64": { pkg: "@bumpyclock/tasque-darwin-x64", bin: "tsq" },
  "linux x64": { pkg: "@bumpyclock/tasque-linux-x64-gnu", bin: "tsq" },
  "linux arm64": { pkg: "@bumpyclock/tasque-linux-arm64-gnu", bin: "tsq" },
  "win32 x64": { pkg: "@bumpyclock/tasque-win32-x64-msvc", bin: "tsq.exe" },
  "win32 arm64": { pkg: "@bumpyclock/tasque-win32-arm64-msvc", bin: "tsq.exe" },
};

function tryResolveBinary() {
  const platformKey = `${process.platform} ${process.arch}`;
  const info = PLATFORMS[platformKey];
  if (!info) return false;

  try {
    const pkgJson = require.resolve(`${info.pkg}/package.json`);
    const binPath = path.join(path.dirname(pkgJson), info.bin);
    return fs.existsSync(binPath);
  } catch {
    return false;
  }
}

function fetch(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return fetch(res.headers.location).then(resolve, reject);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        }
        resolve(res);
      })
      .on("error", reject);
  });
}

// Minimal tar parser — extracts a single file from a tar stream (no deps)
function extractFileFromTar(stream, targetName) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let buf = Buffer.alloc(0);
    let found = false;
    let fileRemaining = 0;
    let fileChunks = [];

    stream.on("data", (chunk) => {
      buf = Buffer.concat([buf, chunk]);

      while (buf.length > 0) {
        if (fileRemaining > 0) {
          const take = Math.min(fileRemaining, buf.length);
          fileChunks.push(buf.subarray(0, take));
          buf = buf.subarray(take);
          fileRemaining -= take;

          if (fileRemaining === 0) {
            // Skip padding to 512-byte boundary
            const pad = (512 - (Buffer.concat(fileChunks).length % 512)) % 512;
            if (buf.length >= pad) {
              buf = buf.subarray(pad);
            }
            if (found) {
              resolve(Buffer.concat(fileChunks));
              stream.destroy();
              return;
            }
            fileChunks = [];
          }
          continue;
        }

        // Need at least 512 bytes for a tar header
        if (buf.length < 512) return;

        const header = buf.subarray(0, 512);
        buf = buf.subarray(512);

        // End of archive: two zero blocks
        if (header.every((b) => b === 0)) {
          break;
        }

        // Parse filename (first 100 bytes, null-terminated)
        const nameEnd = header.indexOf(0, 0);
        const name = header.subarray(0, Math.min(nameEnd >= 0 ? nameEnd : 100, 100)).toString("utf8");

        // Parse file size (octal, bytes 124-136)
        const sizeStr = header.subarray(124, 136).toString("utf8").trim();
        const size = parseInt(sizeStr, 8) || 0;

        // Strip "package/" prefix from npm tarballs
        const entryName = name.replace(/^package\//, "");

        if (entryName === targetName && size > 0) {
          found = true;
          fileRemaining = size;
          fileChunks = [];
        } else {
          // Skip this entry's data + padding
          const totalSize = size + ((512 - (size % 512)) % 512);
          if (buf.length >= totalSize) {
            buf = buf.subarray(totalSize);
          } else {
            fileRemaining = size;
            fileChunks = [];
          }
        }
      }
    });

    stream.on("end", () => {
      if (found && fileChunks.length > 0) {
        resolve(Buffer.concat(fileChunks));
      } else if (!found) {
        reject(new Error(`${targetName} not found in tarball`));
      }
    });

    stream.on("error", reject);
  });
}

async function downloadBinary(pkg, binName, version) {
  const scopedName = pkg.replace("/", "%2f");
  const tarballUrl = `https://registry.npmjs.org/${scopedName}/-/${pkg.split("/")[1]}-${version}.tgz`;

  const binDir = path.join(__dirname, "..", "bin");
  const destPath = path.join(binDir, binName);

  const res = await fetch(tarballUrl);
  const gunzip = res.pipe(createGunzip());
  const data = await extractFileFromTar(gunzip, binName);

  fs.mkdirSync(binDir, { recursive: true });
  fs.writeFileSync(destPath, data);
  fs.chmodSync(destPath, 0o755);

  return destPath;
}

async function main() {
  if (tryResolveBinary()) {
    return;
  }

  const platformKey = `${process.platform} ${process.arch}`;
  const info = PLATFORMS[platformKey];

  if (!info) {
    return;
  }

  const ourPkg = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8")
  );
  const version = ourPkg.version;

  try {
    const dest = await downloadBinary(info.pkg, info.bin, version);
    console.log(`tasque: downloaded ${info.bin} to ${dest}`);
  } catch (err) {
    console.error(
      `Error: Failed to download tsq binary for ${process.platform}-${process.arch}.\n` +
        `${err.message}\n\n` +
        `Try installing the platform package directly:\n` +
        `  npm install ${info.pkg}`
    );
    process.exit(1);
  }
}

main();
