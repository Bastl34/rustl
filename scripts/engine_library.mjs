// the build env of the engine as shared library (crates/rustl_dylib) - the editor (scripts/dev.mjs) and the code of projects (scripts/build.mjs --editor-library) need exactly the same, else cargo builds the engine shared library again
// prefer-dynamic: one std for the editor, the engine shared library and the project shared libraries
// share-generics=n: windows dlls export at most 65535 symbols, the generics rustl shares at opt-level 0 are 44k of them (RUSTC_BOOTSTRAP: -Z on stable)
// .vscode/launch.json has a copy (json can not import it)
export const ENGINE_LIBRARY_ENV =
{
    RUSTC_BOOTSTRAP: "1",
    RUSTFLAGS: "-C prefer-dynamic -Z share-generics=n",
};
