// Usage: node build.js [cross]
// Description: This script builds the frontend and backend and copies the necessary files to the release directory.
// If the 'cross' argument is passed, the backend will be compiled for Windows using the x86_64-pc-windows-gnu target.
// The 'cross' argument is only supported on Linux.
// Install the required dependencies before running this script:
// - Rust
// - Node.js
// - npm
// - Rust mingw toolchain: `rustup target add x86_64-pc-windows-gnu`
// - The `mingw-w64` package on Linux: `sudo apt install mingw-w64` (for the 'cross' argument, Debian-based systems only)
const fs = require('fs');
const process = require("process");
const child_process = require("child_process");
const path = require("path");
const { release } = require('os');
const argv = process.argv.slice(2);

var targetdir = "target/";
targetdir += argv.includes('cross') ? "x86_64-pc-windows-gnu/" : "";
targetdir += "release/";

var releasedir = argv.includes('cross') ? "release-cross/" : "release/";

const copyRecursiveSync = (src, dest) => {
  const exists = fs.existsSync(src);
  const stats = exists && fs.statSync(src);
  const isDirectory = exists && stats.isDirectory();
  if (isDirectory) {
    fs.mkdirSync(dest);
    fs.readdirSync(src).forEach(function (childItemName) {
      copyRecursiveSync(path.join(src, childItemName),
        path.join(dest, childItemName));
    });
  } else {
    fs.copyFileSync(src, dest);
  }
};

const runCommand = (cmd) => {
  console.log("\n", cmd);
  child_process.execSync(cmd, { stdio: 'inherit' });
}

const createReleaseDir = () => {
  if (fs.existsSync(releasedir)) {
    fs.rmSync(releasedir, { recursive: true, force: true });
  }

  releasedir = releasedir + "oasis/";

  fs.mkdirSync(releasedir, { recursive: true }, (e) => {
    if (e) {
      throw e;
    }
  });
}

const filename = process.platform.startsWith("win") || argv.includes('cross') ? "oasis.exe" : "oasis";
const compilecmd = argv.includes('cross') ? "cargo build --release --target x86_64-pc-windows-gnu" : "cargo build --release";

createReleaseDir();

process.chdir("frontend");
runCommand("npm i"); // Install frontend dependencies
runCommand("npm run build"); // Build the frontend
copyRecursiveSync("public", "../" + releasedir + "public"); // Copy the frontend build to the release directory

process.chdir("../backend");
runCommand(compilecmd); // Compile the backend
copyRecursiveSync(targetdir + filename, "../" + releasedir + filename); // Copy the backend to the release directory
copyRecursiveSync("assets/oasis.conf.sample", "../" + releasedir + "oasis.conf.sample"); // Copy the sample configuration file to the release directory

process.chdir("../" + releasedir);
fs.chmodSync(filename, 0o755); // Make the backend executable

console.log("\nBuild complete. Please check the 'release' directory.");