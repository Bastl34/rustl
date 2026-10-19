// the editor with the engine as shared library (crates/rustl_dylib) - it builds the code of a project as shared library and loads it on Play (src/gui/editor/project_code.rs)
// the build env of the shared library: scripts/engine_library.mjs
// crates/rustl_editor is a workspace of its own like the code of projects - both have to build the same engine (see its Cargo.toml)
// usage: cargo dev / dev-debug / dev-release (watches and restarts) | node scripts/dev.mjs [--profile=dev-dylib|debug-dylib|release-dylib] [editor args]

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ENGINE_LIBRARY_ENV } from "./engine_library.mjs";

// the real spelling of the path (drive letter case) - cargo compares the paths of path dependencies as written
const ROOT = fs.realpathSync.native(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."));
const EDITOR_DIR = path.join(ROOT, "crates", "rustl_editor");

const args = process.argv.slice(2);
const profile = args.find(arg => arg.startsWith("--profile="))?.slice("--profile=".length) ?? "dev-dylib";
const editorArgs = args.filter(arg => !arg.startsWith("--profile="));

// the versions of the engine workspace
const lock = path.join(ROOT, "Cargo.lock");
const editorLock = path.join(EDITOR_DIR, "Cargo.lock");
if (!fs.existsSync(editorLock) || fs.statSync(lock).mtimeMs > fs.statSync(editorLock).mtimeMs)
{
    fs.copyFileSync(lock, editorLock);
}

// exactly the flags of the project shared library builds - own RUSTFLAGS would make cargo build the engine again for them
const env = { ...process.env, ...ENGINE_LIBRARY_ENV };

const cargoArgs = ["run", "--manifest-path", path.join(EDITOR_DIR, "Cargo.toml"), "--target-dir", path.join(ROOT, "target"), "--profile", profile, "--", ...editorArgs];
const result = spawnSync("cargo", cargoArgs, { stdio: "inherit", env, cwd: ROOT });
process.exit(result.status ?? 1);
