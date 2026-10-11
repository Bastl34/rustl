// builds without the editor into dist/<platform>, a given project is packed in with every file it uses and starts directly
// web: wasm with threads (nightly + build-std, the atomics flags are in .cargo/config.toml) - windows/linux/mac: native build, has to run on that platform
// usage: npm run build-web|build-windows|build-linux|build-mac -- [--dev] [--out=dir] path/to/x.project   |   npm run dev-web -- path/to/x.project
// --out: another target dir instead of dist/<platform> (the editor export uses it)
// node scripts/build.mjs --editor-library --profile=<profile> --target-dir=<dir> path/to/x.project: the code of the project as shared library for the editor (src/gui/editor/project_code.rs)
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ENGINE_LIBRARY_ENV } from "./engine_library.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const RESOURCE_SCHEME = "resources://";
const SETUP_WEB = "one time setup: rustup toolchain install nightly && rustup +nightly target add wasm32-unknown-unknown && rustup +nightly component add rust-src && cargo install wasm-pack";

// the packaged project inside the resources of the build - the engine starts project/app.project (main_interface.rs)
const BUNDLE_DIR = "project";
const BUNDLE_PROJECT = "app.project";

// the code of a project: <project folder>/code (src/gui/editor/project_code.rs) - never packed, the build links it in
const CODE_DIR = "code";

// what the engine itself loads from resources/ without the editor (resources/engine_resources.txt) - everything else only goes in when the project uses it (resources://)
const ENGINE_RESOURCES = fs.readFileSync(path.join(ROOT, "resources", "engine_resources.txt"), "utf8").split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith("#"));

// the rustl logo for now - later the app icon of the project
const APP_ICON = path.join(ROOT, "resources", "designs", "logo", "logo.png");

// host: the platform a native build has to run on (cross compiling needs the linker and SDK of the target)
const PLATFORMS =
{
    web: { dir: "dist/web" },
    windows: { host: "win32", dir: "dist/windows" },
    linux: { host: "linux", dir: "dist/linux" },
    mac: { host: "darwin", dir: "dist/mac" },
};

const args = process.argv.slice(2);
const dev = args.includes("--dev");
const watch = args.includes("--watch");
const platformName = (args.find(arg => arg.startsWith("--platform=")) ?? "--platform=web").split("=")[1];
const platform = PLATFORMS[platformName];

// npm runs scripts in the package root - INIT_CWD is where it was called from
const projectArg = args.find(arg => !arg.startsWith("--"));
const project = projectArg ? path.resolve(process.env.INIT_CWD ?? process.cwd(), projectArg) : null;
const outArg = args.find(arg => arg.startsWith("--out="))?.slice("--out=".length).trim();
const outDir = outArg ? path.resolve(process.env.INIT_CWD ?? process.cwd(), outArg) : null;
const editorLibrary = args.includes("--editor-library");
const profileArg = args.find(arg => arg.startsWith("--profile="))?.slice("--profile=".length);
const targetDirArg = args.find(arg => arg.startsWith("--target-dir="))?.slice("--target-dir=".length);

// ******************** commands ********************

function build()
{
    try
    {
        if (platform.host && process.platform !== platform.host)
        {
            throw new Error(`build-${platformName} has to run on ${platformName} - cross compiling needs the linker and SDK of that platform`);
        }

        if (!fs.existsSync(project))
        {
            throw new Error(`project not found: ${project}`);
        }

        const code = projectCode(project);

        const name = appName();
        const target = layout(name);

        fs.mkdirSync(target.dir, { recursive: true });

        // a custom dir can hold other files of the user - only dist/ is cleaned up
        if (!outDir)
        {
            removeOtherApps(target);
        }

        // packaged before the long build, so a broken project fails fast
        const resources = new Resources(target.resources);
        for (const entry of ENGINE_RESOURCES)
        {
            resources.add(entry);
        }

        const bundleDir = path.join(target.resources, BUNDLE_DIR);
        fs.rmSync(bundleDir, { recursive: true, force: true });
        const startProject = packageProject(project, bundleDir, resources, target.dir);

        resources.sync();

        if (platformName === "web")
        {
            writeWebFiles(target.dir, project);
            buildWeb(target.dir, code);
            const served = path.relative(ROOT, target.dir).replaceAll("\\", "/");
            console.log(outDir ? `web build ready in ${target.dir}, starts ${startProject} - serve that dir with COOP/COEP headers (serve.json)` : `web build ready in ${served}, starts ${startProject} - http://localhost:1337/${served}/ (npx serve -p 1337 in the repo root)`);
        }
        else
        {
            buildNative(target, name, code);
            console.log(`${platformName} build ready: ${path.relative(ROOT, target.app)}, starts ${startProject}`);
        }

        return true;
    }
    catch (err)
    {
        console.error(`error: ${err.message}`);
        return false;
    }
}

function buildWeb(dir, code)
{
    // own target dir, so web builds do not wait for the file locks of native builds or rust-analyzer
    // a project with code: its crate is the wasm module, the engine takes the app from its rustl_create_app (RUSTL_EXTERNAL_START, build.rs)
    // --out-name: web/index.html loads pkg/rustl.js either way - after the path and -- everything goes to cargo
    // --no-opt: the binaryen wasm-pack downloads is too old for the threaded build (table.fill) - needs wasm-pack 0.12+
    const pkg = path.join(dir, "pkg");
    const crateDir = code ? code.dir : ROOT;
    const args = ["run", "nightly", "wasm-pack", "build", "--target", "web", "--no-typescript", "--no-opt", "--out-dir", pkg, "--out-name", "rustl", ...(dev ? ["--dev"] : []), crateDir, "--", "--no-default-features", "-Z", "build-std=std,panic_abort", ...(code ? engineCargoArgs(code.dir) : [])];
    const env = { CARGO_TARGET_DIR: path.join(ROOT, "target", "web"), ...(code ? { RUSTL_EXTERNAL_START: "1" } : {}) };
    // wasm-pack runs cargo metadata on its own (without the engine args) and takes the wasm-bindgen cli version from it
    // -> the lock of the project lies in the code folder while it builds, so it sees the versions the wasm is built with (removed afterwards)
    const folderLock = code ? path.join(code.dir, "Cargo.lock") : null;
    const ownLock = folderLock !== null && fs.existsSync(folderLock);
    if (folderLock && !ownLock)
    {
        fs.copyFileSync(prepareLock(code.dir), folderLock);
    }

    try
    {
        run("rustup", args, env, SETUP_WEB);
    }
    finally
    {
        if (folderLock && !ownLock)
        {
            fs.rmSync(folderLock, { force: true });
        }
    }

    // wasm-pack builds an npm package - the page only needs the .js, the .wasm and snippets/
    for (const entry of fs.readdirSync(pkg))
    {
        if ([".gitignore", "package.json", "README.md"].includes(entry) || entry.startsWith("LICENSE") || entry.endsWith(".d.ts"))
        {
            fs.rmSync(path.join(pkg, entry), { force: true });
        }
    }
}

function buildNative(target, name, code)
{
    // own target dir, so the build without editor does not replace the one of cargo run
    // a project with code: its own executable, the code linked in (without the engine shared library of the editor)
    const targetDir = path.join(ROOT, "target", "dist");
    const engineArgs = code ? engineCargoArgs(code.dir) : [];
    const packageArgs = code ? ["--manifest-path", path.join(code.dir, "Cargo.toml"), "--bin", code.crate] : [];
    run("cargo", [...engineArgs, "build", ...packageArgs, "--no-default-features", ...(dev ? [] : ["--release"])], { CARGO_TARGET_DIR: targetDir });

    const exe = code ? code.crate : "rustl";
    fs.mkdirSync(path.dirname(target.exe), { recursive: true });
    fs.copyFileSync(path.join(targetDir, dev ? "debug" : "release", process.platform === "win32" ? `${exe}.exe` : exe), target.exe);
    fs.chmodSync(target.exe, 0o755);

    // the exe is named like the project already - windows shows that name without a version resource
    if (platformName === "windows")
    {
        run("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", path.join(ROOT, "scripts", "windows_icon.ps1"), "-Exe", target.exe, "-Png", APP_ICON]);
    }

    if (platformName === "mac")
    {
        writeMacIcon(target.app);
        writeInfoPlist(target.app, name);
    }
}

// the code of the project as shared library for the editor: the profile and target dir of the running editor, so the engine shared library is shared and not built again
// cargo's json messages go straight through to the editor (stdout), it reads the diagnostics and the shared library from them
function buildEditorLibrary()
{
    const code = projectCode(project);
    if (!code)
    {
        console.error(`error: the project has no code: ${project}`);
        return false;
    }

    const cargoArgs = [...engineCargoArgs(code.dir), "rustc", "--lib", "--crate-type", "dylib", "--message-format=json", "--color", "never", "-p", code.crate, "--profile", profileArg, "--target-dir", targetDirArg, "--manifest-path", path.join(code.dir, "Cargo.toml")];
    const result = spawnSync("cargo", cargoArgs, { cwd: code.dir, stdio: "inherit", env: buildEnv(ENGINE_LIBRARY_ENV) });
    if (result.error)
    {
        console.error(`error: failed to run cargo: ${result.error.message}`);
    }

    return result.status === 0;
}

// <project folder>/code with its crate name - null without code (src/gui/editor/project_code.rs)
function projectCode(projectFile)
{
    // the real spelling (drive letter case): cargo resolves the relative engine path from it - another spelling would be another engine build
    const dir = path.join(path.dirname(fs.realpathSync.native(projectFile)), CODE_DIR);
    const manifest = path.join(dir, "Cargo.toml");
    if (!fs.existsSync(manifest))
    {
        return null;
    }

    const crate = readText(manifest).match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    if (!crate)
    {
        throw new Error(`no package name in ${manifest}`);
    }

    return { dir, crate };
}

// the code of a project is a workspace of its own - the cargo args for the config of the engine (profiles, egui patch) and its lock (.cargo/Cargo.lock) - also for the shared library of the editor (--editor-library)
function engineCargoArgs(dir)
{
    const slashes = (file) => file.replaceAll("\\", "/");
    return ["--config", slashes(fs.realpathSync.native(path.join(ROOT, ".cargo", "config.toml"))), "--config", `resolver.lockfile-path='${slashes(prepareLock(dir))}'`];
}

// .cargo/Cargo.lock of the project: the entries of the engine plus the crates only the project uses - by content, not by date: an engine update always arrives
function prepareLock(dir)
{
    const read = (file) => fs.existsSync(file) ? fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n") : null;
    const target = path.join(dir, ".cargo", "Cargo.lock");

    const lock = read(path.join(ROOT, "Cargo.lock"));
    const projectLock = read(target);
    const merged = projectLock === null ? lock : mergeLock(lock, projectLock);
    if (merged !== null)
    {
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, merged);
    }

    return target;
}

// header and [[package]] blocks of a Cargo.lock - the key is name + version + source
function lockBlocks(text)
{
    const [header, ...parts] = text.split("\n[[package]]\n");
    const blocks = parts.map(part =>
    {
        const block = part.trimEnd();
        const key = block.split("\n").filter(line => line.startsWith("name = ") || line.startsWith("version = ") || line.startsWith("source = ")).join("|");
        return { key, block };
    });
    return { header: header.trimEnd(), blocks };
}

// null: every entry of the engine is in the project lock as it is
function mergeLock(engine, project)
{
    const engineLock = lockBlocks(engine);
    const projectLock = lockBlocks(project);

    const projectTexts = new Set(projectLock.blocks.map(entry => entry.block));
    if (engineLock.blocks.every(entry => projectTexts.has(entry.block)))
    {
        return null;
    }

    const engineKeys = new Set(engineLock.blocks.map(entry => entry.key));
    const blocks = [...engineLock.blocks, ...projectLock.blocks.filter(entry => !engineKeys.has(entry.key))];
    blocks.sort((a, b) => a.key < b.key ? -1 : a.key > b.key ? 1 : 0);

    return engineLock.header + "\n" + blocks.map(entry => "\n[[package]]\n" + entry.block + "\n").join("");
}

// native builds are named like the project (its name, else the file name)
function appName()
{
    return projectName(project).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").replace(/[. ]+$/, "") || "rustl";
}

// the project section of a .project file - empty if it is broken (the packaging reports that)
function projectInfo(file)
{
    try
    {
        return JSON.parse(readText(file)).project ?? {};
    }
    catch
    {
        return {};
    }
}

// its name, else the file name
function projectName(file)
{
    const name = projectInfo(file).name?.trim();
    return name && name !== "Untitled" ? name : path.basename(file, path.extname(file));
}

// where the executable and the resources go
function layout(name)
{
    const dir = outDir ?? path.join(ROOT, platform.dir);

    switch (platformName)
    {
        case "windows": return { dir, app: path.join(dir, `${name}.exe`), exe: path.join(dir, `${name}.exe`), resources: path.join(dir, "resources") };
        case "linux": return { dir, app: path.join(dir, name), exe: path.join(dir, name), resources: path.join(dir, "resources") };
        case "mac":
        {
            const app = path.join(dir, `${name}.app`);
            return { dir, app, exe: path.join(app, "Contents", "MacOS", name), resources: path.join(app, "Contents", "Resources", "resources") };
        }
        default: return { dir, resources: path.join(dir, "resources") };
    }
}

// a build of another project would leave its executable behind otherwise
function removeOtherApps(target)
{
    if (!target.app || !fs.existsSync(target.dir))
    {
        return;
    }

    for (const entry of fs.readdirSync(target.dir))
    {
        if (entry !== "resources" && entry !== path.basename(target.app))
        {
            fs.rmSync(path.join(target.dir, entry), { recursive: true, force: true });
        }
    }
}

// the build flags come from .cargo/config.toml - an inherited build env overrides them (the editor of cargo dev: RUSTFLAGS without atomics + prefer-dynamic, cargo run: CARGO_*)
const INHERITED_BUILD_ENV = /^(RUSTFLAGS|RUSTC_BOOTSTRAP|RUSTUP_TOOLCHAIN|OUT_DIR|CARGO|CARGO_(?!HOME$|TERM_)\w+)$/i;

function buildEnv(env)
{
    const inherited = Object.fromEntries(Object.entries(process.env).filter(([key]) => !INHERITED_BUILD_ENV.test(key)));
    return { ...inherited, ...env };
}

function run(command, commandArgs, env = {}, setup = "", stdio = "inherit")
{
    const result = spawnSync(command, commandArgs, { cwd: ROOT, stdio, env: buildEnv(env) });
    if (result.error)
    {
        throw new Error(`failed to run ${command}: ${result.error.message}${setup ? `\n${setup}` : ""}`);
    }
    if (result.status !== 0)
    {
        throw new Error(`${command} ${commandArgs.slice(0, 2).join(" ")} failed`);
    }
}

// dev builds on every change of the code, the resources, the web files or the project
function watchAndBuild()
{
    const watched = [path.join(ROOT, "src"), path.join(ROOT, "resources"), path.join(ROOT, "web")];

    // the whole project folder, unless that would also watch dist/ and target/
    const projectDir = path.dirname(project);
    const rootFromProject = path.relative(projectDir, ROOT);
    watched.push(rootFromProject.startsWith("..") || path.isAbsolute(rootFromProject) ? projectDir : project);

    // builds run synchronously - changes made meanwhile arrive afterwards and trigger the next build
    let timer = null;
    let buildStarted = 0;
    const rebuild = (reason) =>
    {
        clearTimeout(timer);
        timer = setTimeout(() =>
        {
            console.log(`\n[${reason}] building...`);
            buildStarted = Date.now();
            build();
            console.log("watching for changes...");
        }, 300);
    };

    // windows also reports reads (rustc reading the sources) - only files written since the last build count
    const modified = (file) =>
    {
        try { return fs.statSync(file).mtimeMs >= buildStarted; }
        catch { return true; }
    };

    for (const target of watched)
    {
        const recursive = fs.statSync(target).isDirectory();
        fs.watch(target, { recursive }, (_event, file) =>
        {
            const changed = recursive ? path.join(target, file ?? "") : target;
            if (modified(changed))
            {
                rebuild(path.relative(ROOT, changed));
            }
        });
    }

    rebuild("start");
}

// ******************** platform files ********************

function writeWebFiles(dist, projectFile)
{
    const template = fs.readFileSync(path.join(ROOT, "web", "index.html"), "utf8");
    const html = template.replace("<title>Rustl</title>", webHead(projectFile));
    fs.writeFileSync(path.join(dist, "index.html"), html);
    fs.copyFileSync(APP_ICON, path.join(dist, "favicon.png"));
}

// title, description and the link preview of slack, discord, messengers (open graph + twitter card) - they need an absolute image url, project.url is where the build is hosted
function webHead(projectFile)
{
    const info = projectInfo(projectFile);
    const title = projectName(projectFile);
    const description = (info.description ?? "").replace(/\s+/g, " ").trim();
    const author = (info.author ?? "").trim();
    const url = (info.url ?? "").trim();
    const [width, height] = pngSize(APP_ICON);

    let image = "favicon.png";
    if (url)
    {
        try
        {
            image = new URL(image, /\/$|\.html?$/i.test(url) ? url : `${url}/`).href;
        }
        catch
        {
            console.warn(`warning: project url ${url} is not absolute - link previews will miss the image`);
        }
    }
    else if (!dev)
    {
        console.warn("warning: the project has no url - link previews (slack, discord...) need it for the image");
    }

    const attr = (text) => text.replaceAll("&", "&amp;").replaceAll("\"", "&quot;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

    return [
        `<title>${attr(title)}</title>`,
        description && `<meta name="description" content="${attr(description)}">`,
        author && `<meta name="author" content="${attr(author)}">`,
        `<meta property="og:type" content="website">`,
        `<meta property="og:title" content="${attr(title)}">`,
        description && `<meta property="og:description" content="${attr(description)}">`,
        url && `<meta property="og:url" content="${attr(url)}">`,
        `<meta property="og:image" content="${attr(image)}">`,
        `<meta property="og:image:width" content="${width}">`,
        `<meta property="og:image:height" content="${height}">`,
        `<meta name="twitter:card" content="summary">`,
    ].filter(Boolean).join("\n    ");
}

// width and height from the IHDR chunk
function pngSize(file)
{
    const data = fs.readFileSync(file);
    return [data.readUInt32BE(16), data.readUInt32BE(20)];
}

// icns from the app icon, with the tools macOS brings along
function writeMacIcon(app)
{
    const iconset = path.join(ROOT, "target", "dist", "AppIcon.iconset");
    fs.rmSync(iconset, { recursive: true, force: true });
    fs.mkdirSync(iconset, { recursive: true });

    for (const size of [16, 32, 128, 256, 512])
    {
        for (const [scale, suffix] of [[1, ""], [2, "@2x"]])
        {
            const pixels = String(size * scale);
            run("sips", ["-z", pixels, pixels, APP_ICON, "--out", path.join(iconset, `icon_${size}x${size}${suffix}.png`)], {}, "", ["ignore", "ignore", "inherit"]);
        }
    }

    run("iconutil", ["-c", "icns", iconset, "-o", path.join(app, "Contents", "Resources", "AppIcon.icns")]);
}

function writeInfoPlist(app, name)
{
    const version = fs.readFileSync(path.join(ROOT, "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "0.0.1";
    const xml = (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
    const identifier = `com.rustl.${name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "app"}`;

    const plist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>${xml(name)}</string>
    <key>CFBundleIdentifier</key><string>${identifier}</string>
    <key>CFBundleName</key><string>${xml(name)}</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${version}</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
`;
    fs.writeFileSync(path.join(app, "Contents", "Info.plist"), plist);
}

function needsCopy(src, dst)
{
    if (!fs.existsSync(dst))
    {
        return true;
    }

    const from = fs.statSync(src);
    const to = fs.statSync(dst);
    return from.size !== to.size || from.mtimeMs > to.mtimeMs;
}

// ******************** resources ********************

// the files of resources/ the build needs - copied when changed, everything else is removed from the build
class Resources
{
    constructor(dir)
    {
        this.dir = dir;
        this.source = fs.realpathSync.native(path.join(ROOT, "resources"));
        this.files = new Set();
    }

    // a file (with the files it references itself) or a whole folder, relative to resources/
    add(relative, usedBy = "the engine")
    {
        let file;
        try
        {
            // the real spelling - on windows a differently cased path would not match when pruning
            file = fs.realpathSync.native(path.join(this.source, relative));
        }
        catch
        {
            console.warn(`warning: missing resource ${relative} (used by ${usedBy})`);
            return;
        }

        const key = path.relative(this.source, file).replaceAll("\\", "/");
        if (key.startsWith("..") || path.isAbsolute(key))
        {
            console.warn(`warning: ${file} is not inside resources/ (used by ${usedBy})`);
            return;
        }

        if (fs.statSync(file).isDirectory())
        {
            for (const entry of fs.readdirSync(file))
            {
                this.add(path.join(key, entry), usedBy);
            }
            return;
        }

        if (this.files.has(key))
        {
            return;
        }

        this.files.add(key);
        for (const referenced of referencedFiles(file))
        {
            this.add(path.relative(this.source, referenced), file);
        }
    }

    sync()
    {
        for (const key of this.files)
        {
            const target = path.join(this.dir, key);
            if (needsCopy(path.join(this.source, key), target))
            {
                fs.mkdirSync(path.dirname(target), { recursive: true });
                fs.copyFileSync(path.join(this.source, key), target);
            }
        }

        this.prune(this.dir, "");
    }

    // the packaged project is written separately
    prune(dir, relative)
    {
        for (const entry of fs.readdirSync(dir, { withFileTypes: true }))
        {
            const key = relative ? `${relative}/${entry.name}` : entry.name;
            const full = path.join(dir, entry.name);

            if (!relative && entry.name === BUNDLE_DIR)
            {
                continue;
            }

            if (entry.isDirectory())
            {
                this.prune(full, key);
                if (fs.readdirSync(full).length === 0)
                {
                    fs.rmdirSync(full);
                }
            }
            else if (!this.files.has(key))
            {
                fs.rmSync(full);
            }
        }
    }
}

// ******************** project packaging ********************

// copies the project into the bundle dir and rewrites the paths that point outside of it, resources:// files go to the resources - returns the project path for the build
// a project in a folder of its own: the whole folder (without code/ and hidden entries) - the code of the project may load any file of it
// a project next to others (old layout): only its scenes and the files they use
function packageProject(projectPath, bundleDir, resources, outputDir)
{
    const projectFile = fs.realpathSync.native(projectPath);
    const projectDir = path.dirname(projectFile);
    const projectJson = JSON.parse(readText(projectFile));
    const bundle = new Bundle(bundleDir, projectDir);
    let sceneCount = 0;

    fs.mkdirSync(bundleDir, { recursive: true });

    const ownFolder = fs.readdirSync(projectDir).filter(entry => entry.toLowerCase().endsWith(".project")).length === 1;
    if (ownFolder)
    {
        bundle.addFolder(projectDir, [projectFile, path.join(projectDir, CODE_DIR), outputDir]);
    }
    else
    {
        console.warn(`warning: ${projectDir} holds other projects too - only the files of the scenes are packed (a project in a folder of its own gets the whole folder)`);
    }

    for (const sceneRef of projectJson.scenes ?? [])
    {
        const scenePath = fs.realpathSync.native(path.resolve(projectDir, sceneRef.path));
        const text = readText(scenePath);
        const sceneDir = path.dirname(scenePath);
        const scenePacked = bundle.bundlePath(scenePath);

        // old -> new source, relative to the packed scene - only these strings change, everything else stays as saved
        const sources = new Map();
        for (const source of sceneSources(JSON.parse(text)))
        {
            if (source.startsWith(RESOURCE_SCHEME))
            {
                resources.add(source.slice(RESOURCE_SCHEME.length), scenePath);
            }
            else if (!sources.has(source))
            {
                const packed = bundle.add(path.resolve(sceneDir, source), scenePath);
                if (packed)
                {
                    sources.set(source, path.posix.relative(path.posix.dirname(scenePacked), packed));
                }
            }
        }

        const rewritten = text.replace(/("source"\s*:\s*)("(?:[^"\\]|\\.)*")/g, (match, key, value) =>
        {
            const packed = sources.get(JSON.parse(value));
            return packed ? key + JSON.stringify(packed) : match;
        });

        const target = path.join(bundleDir, scenePacked);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, rewritten);
        sceneRef.path = scenePacked;
        sceneCount++;
    }

    fs.writeFileSync(path.join(bundleDir, BUNDLE_PROJECT), JSON.stringify(projectJson, null, 2) + "\n");

    console.log(`packaged ${path.basename(projectFile)} (${sceneCount} scenes, ${bundle.copied.size} files${ownFolder ? ", the whole project folder" : ""})`);

    return `${BUNDLE_DIR}/${BUNDLE_PROJECT}`;
}

// object sources (also of child objects) and sound sources - relative to the scene file or resources://
function sceneSources(scene)
{
    const sources = [];
    const addObjects = (objects) =>
    {
        for (const object of objects ?? [])
        {
            if (object.source)
            {
                sources.push(object.source);
            }
            addObjects(object.objects);
        }
    };

    addObjects(scene.objects);

    for (const sound of scene.sounds ?? [])
    {
        if (sound.source)
        {
            sources.push(sound.source);
        }
    }

    return sources;
}

class Bundle
{
    constructor(dir, projectDir)
    {
        this.dir = dir;
        this.projectDir = projectDir;
        this.root = fs.realpathSync.native(ROOT);
        this.copied = new Map();
    }

    // the project folder is the bundle dir - files outside of it go to external/ (engine files by their path in the repo, others by their path on the disk)
    // the layout stays, so relative references between files still work
    bundlePath(file)
    {
        const outside = (relative) => relative.startsWith("..") || path.isAbsolute(relative);

        let relative = path.relative(this.projectDir, file);
        if (outside(relative))
        {
            const inRepo = path.relative(this.root, file);
            relative = path.join("external", outside(inRepo) ? file.replace(/^[a-zA-Z]:/, "").replace(/^[\\/]+/, "") : inRepo);
        }

        return relative.replaceAll("\\", "/");
    }

    // every file of a folder - without hidden entries, the skipped paths and sub folders of other projects (backups, ...)
    addFolder(dir, skip)
    {
        for (const entry of fs.readdirSync(dir, { withFileTypes: true }))
        {
            const full = path.join(dir, entry.name);
            if (entry.name.startsWith(".") || skip.some(skipped => skipped && path.relative(skipped, full) === ""))
            {
                continue;
            }

            if (entry.isDirectory())
            {
                if (!fs.readdirSync(full).some(name => name.toLowerCase().endsWith(".project")))
                {
                    this.addFolder(full, skip);
                }
            }
            else if (entry.isFile())
            {
                this.copy(fs.realpathSync.native(full));
            }
        }
    }

    copy(real)
    {
        const packed = this.bundlePath(real);
        const target = path.join(this.dir, packed);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.copyFileSync(real, target);
        this.copied.set(real, packed);
        return packed;
    }

    // copies a file and the files it references itself (gltf buffers/images, obj materials, mtl textures) - null if it is missing
    add(file, usedBy)
    {
        let real;
        try
        {
            real = fs.realpathSync.native(file);
        }
        catch
        {
            console.warn(`warning: missing file ${file} (used by ${usedBy})`);
            return null;
        }

        if (this.copied.has(real))
        {
            return this.copied.get(real);
        }

        const packed = this.copy(real);
        for (const referenced of referencedFiles(real))
        {
            this.add(referenced, real);
        }

        return packed;
    }
}

// ******************** asset dependencies ********************

// the files a file references itself, as absolute paths
function referencedFiles(file)
{
    const dir = path.dirname(file);

    // mtl texture lines can have options in front of the file name
    return dependencies(file).map(dependency => fs.existsSync(path.resolve(dir, dependency)) ? path.resolve(dir, dependency) : path.resolve(dir, dependency.split(/\s+/).pop()));
}

function dependencies(file)
{
    const extension = path.extname(file).toLowerCase();

    switch (extension)
    {
        case ".gltf": return gltfUris(readText(file));
        case ".glb": return gltfUris(glbJson(fs.readFileSync(file)));
        case ".obj": return lineValues(readText(file), keyword => keyword === "mtllib");
        case ".mtl": return lineValues(readText(file), keyword => keyword.startsWith("map_") || ["bump", "norm", "disp", "decal", "refl"].includes(keyword));
        default: return [];
    }
}

function gltfUris(json)
{
    let gltf;
    try
    {
        gltf = JSON.parse(json);
    }
    catch
    {
        return [];
    }

    return [...(gltf.buffers ?? []), ...(gltf.images ?? [])]
        .map(item => item.uri)
        .filter(uri => uri && !uri.startsWith("data:"))
        .map(uri =>
        {
            try { return decodeURIComponent(uri); }
            catch { return uri; }
        });
}

// the JSON chunk of a binary gltf
function glbJson(data)
{
    if (data.length < 20 || data.toString("latin1", 0, 4) !== "glTF" || data.toString("latin1", 16, 20) !== "JSON")
    {
        return "";
    }

    return data.toString("utf8", 20, 20 + data.readUInt32LE(12));
}

// like tobj: the rest of the line is the file name (it can contain spaces)
function lineValues(text, isFileKeyword)
{
    return text.split(/\r?\n/)
        .map(line => line.trim().match(/^(\S+)\s+(.+)$/))
        .filter(match => match && isFileKeyword(match[1]))
        .map(match => match[2].trim());
}

function readText(file)
{
    return fs.readFileSync(file, "utf8").replace(/^\uFEFF/, "");
}

// ******************** main ********************

if (!platform)
{
    console.error(`error: unknown platform ${platformName} (${Object.keys(PLATFORMS).join(", ")})`);
    process.exit(1);
}

if (!project)
{
    console.error(`error: no project given - npm run ${watch ? "dev" : "build"}-${platformName} -- path/to/x.project`);
    process.exit(1);
}

if (editorLibrary)
{
    if (!profileArg || !targetDirArg)
    {
        console.error("error: --editor-library needs --profile and --target-dir");
        process.exit(1);
    }
    process.exit(buildEditorLibrary() ? 0 : 1);
}
else if (watch)
{
    watchAndBuild();
}
else
{
    process.exit(build() ? 0 : 1);
}
