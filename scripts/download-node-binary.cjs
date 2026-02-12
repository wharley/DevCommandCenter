/**
 * Downloads the Node.js binary for the current platform/arch and places it
 * in electron/resources/bin/ for electron-builder extraResources.
 * Run before electron:build (e.g. "node scripts/download-node-binary.cjs").
 *
 * Node version is aligned with Electron 33 (Node 22.x LTS).
 */

const fs = require("fs");
const path = require("path");
const https = require("https");
const { execSync } = require("child_process");

const NODE_VERSION = "v22.22.0";
const BASE_URL = `https://nodejs.org/dist/${NODE_VERSION}`;
const OUT_DIR = path.join(__dirname, "..", "electron", "resources", "bin");

function getPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "darwin") return { platform: "darwin", arch };
  if (platform === "win32") return { platform: "win", arch: "x64" };
  if (platform === "linux") return { platform: "linux", arch: "x64" };
  throw new Error(`Unsupported platform: ${platform}`);
}

function getDownloadInfo() {
  const { platform, arch } = getPlatformKey();
  if (platform === "darwin") {
    const suffix = arch === "arm64" ? "darwin-arm64" : "darwin-x64";
    return {
      url: `${BASE_URL}/node-${NODE_VERSION}-${suffix}.tar.gz`,
      archiveName: `node-${NODE_VERSION}-${suffix}.tar.gz`,
      extractDir: `node-${NODE_VERSION}-${suffix}`,
      binaryPath: "bin/node",
      outName: "node",
      isZip: false,
    };
  }
  if (platform === "win") {
    return {
      url: `${BASE_URL}/node-${NODE_VERSION}-win-x64.zip`,
      archiveName: `node-${NODE_VERSION}-win-x64.zip`,
      extractDir: `node-${NODE_VERSION}-win-x64`,
      binaryPath: "node.exe",
      outName: "node.exe",
      isZip: true,
    };
  }
  if (platform === "linux") {
    return {
      url: `${BASE_URL}/node-${NODE_VERSION}-linux-x64.tar.xz`,
      archiveName: `node-${NODE_VERSION}-linux-x64.tar.xz`,
      extractDir: `node-${NODE_VERSION}-linux-x64`,
      binaryPath: "bin/node",
      outName: "node",
      isZip: false,
    };
  }
  throw new Error(`Unsupported: ${platform}`);
}

function download(url) {
  return new Promise((resolve, reject) => {
    const file = path.join(require("os").tmpdir(), path.basename(url));
    const stream = fs.createWriteStream(file);
    https
      .get(url, { headers: { "User-Agent": "DevCommandCenter-build" } }, (res) => {
        if (res.statusCode === 302 || res.statusCode === 301) {
          const redirect = res.headers.location;
          if (redirect) {
            stream.close();
            fs.unlink(file, () => {});
            return download(redirect).then(resolve).catch(reject);
          }
        }
        if (res.statusCode !== 200) {
          stream.close();
          fs.unlink(file, () => {});
          return reject(new Error(`HTTP ${res.statusCode}: ${url}`));
        }
        res.pipe(stream);
        stream.on("finish", () => {
          stream.close(() => resolve(file));
        });
      })
      .on("error", (err) => {
        stream.close();
        fs.unlink(file, () => {});
        reject(err);
      });
  });
}

function main() {
  const info = getDownloadInfo();
  console.log(`Downloading Node ${NODE_VERSION} for ${process.platform}-${process.arch}...`);
  console.log(info.url);

  fs.mkdirSync(OUT_DIR, { recursive: true });

  download(info.url)
    .then((archivePath) => {
      const extractTo = path.join(require("os").tmpdir(), `node-extract-${Date.now()}`);
      fs.mkdirSync(extractTo, { recursive: true });

      try {
        if (info.isZip) {
          if (process.platform === "win32") {
            execSync(
              `powershell -NoProfile -Command "Expand-Archive -Path '${archivePath.replace(/'/g, "''")}' -DestinationPath '${extractTo.replace(/'/g, "''")}' -Force"`,
              { stdio: "inherit" }
            );
          } else {
            execSync(`unzip -o -q "${archivePath}" -d "${extractTo}"`, {
              stdio: "inherit",
            });
          }
        } else {
          const tarFlags = info.archiveName.endsWith(".tar.xz") ? "-xJf" : "-xzf";
          execSync(`tar ${tarFlags} "${archivePath}" -C "${extractTo}"`, {
            stdio: "inherit",
          });
        }

        const extractedBinary = path.join(extractTo, info.extractDir, info.binaryPath);
        const destPath = path.join(OUT_DIR, info.outName);
        if (!fs.existsSync(extractedBinary)) {
          throw new Error(`Expected binary not found: ${extractedBinary}`);
        }
        fs.copyFileSync(extractedBinary, destPath);
        if (process.platform !== "win32") {
          fs.chmodSync(destPath, 0o755);
        }
        console.log(`Done. Node binary at ${destPath}`);
      } finally {
        try {
          fs.rmSync(extractTo, { recursive: true, force: true });
        } catch (_) {}
        try {
          fs.unlinkSync(archivePath);
        } catch (_) {}
      }
    })
    .catch((err) => {
      console.error(err);
      process.exit(1);
    });
}

main();
