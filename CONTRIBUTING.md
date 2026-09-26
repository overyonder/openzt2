# Contributing

These rules apply to all OpenZT2 contributions.

## Building

### Toolchain and libraries

We prefer the Nix flake for development. It supplies the compiler, native
libraries and tools, including our patched shader translators. The flake
currently supports x86_64 Linux; native builds are described below.

| Dependency | Version |
| --- | --- |
| Rust | 1.96.0 |
| Bevy | 0.19.1 |
| Avian3D | 0.7.0 |
| Hanabi | 0.19.0 |
| wgpu | 29.0.4 |
| mlua | 0.11.6 (Lua 5.1) |
| lunify | 1.1.0 |

`Cargo.lock` records the Rust dependency versions. `flake.lock` pins the Nix
inputs and the vkd3d and MojoShader sources; their patches are in `patches/`.

### Linux with Nix

Install Nix with flakes enabled, direnv and nix-direnv, with the direnv hook
configured for your shell. From the repository root:

```sh
export KAI_CARGO_TARGET_ROOT="$HOME/.cache/cargo-targets"
direnv allow .
direnv exec . cargo check --locked --workspace --all-targets
direnv exec . cargo run --locked --bin openzt2
```

`KAI_CARGO_TARGET_ROOT` chooses the parent directory for Cargo's build cache.
Keep it set in your shell. The first activation builds the patched native
libraries and can take a while.

Running the game requires your own Zoo Tycoon 2 files. The Nix environment
looks in `vendor/original-software` by default; `OPENZT2_Z2F_PATH` can point to
another directory. Original game files must stay out of Git.

For an optimised development build, add `--profile profiling` to `cargo run`.
For the shipping binary, use `direnv exec . cargo build --locked --release
-p openzt2_game --bin openzt2`. Release linking takes considerably longer.

### Native Linux and macOS

The native release targets are Ubuntu 24.04 x86_64 and macOS on Apple
Silicon. Install Git, Python 3.12, rustup and direnv 2.37.1.
macOS also needs Xcode Command Line Tools and Homebrew.

On Ubuntu, install the native dependencies:

```sh
sudo apt-get install build-essential autoconf automake libtool flex bison \
    libjson-perl cmake ninja-build pkg-config spirv-headers libvulkan-dev \
    libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
    libfontconfig1-dev patchelf dpkg-dev mingw-w64-tools
```

On macOS:

```sh
brew install autoconf automake libtool flex bison cmake ninja pkg-config \
    spirv-headers vulkan-headers cpanminus gpatch mingw-w64
cpanm --local-lib="$HOME/.local/openzt2-perl" JSON
export PERL5LIB="$HOME/.local/openzt2-perl/lib/perl5${PERL5LIB:+:$PERL5LIB}"
export PATH="$(brew --prefix flex)/bin:$(brew --prefix bison)/bin:$PATH"
export CPPFLAGS="-I$(brew --prefix)/include"
```

Then, from the repository root on either platform:

```sh
rustup toolchain install 1.96.0 --profile minimal --component rustfmt,clippy
rustup override set 1.96.0
export OPENZT2_RELEASE_HOST=1
export OPENZT2_NATIVE_DEPENDENCY_DIR="$PWD/build/native"
export KAI_CARGO_TARGET_ROOT="$HOME/.cache/cargo-targets"
bash tools/release/build-native.sh
direnv allow .
direnv exec . cargo build --locked -p openzt2_game --bin openzt2
```

The native dependency script needs a fresh staging directory. Once it has
succeeded, reuse the same directory for subsequent Cargo builds; rerun the
script with a fresh prefix when the native sources or patches change.

To produce a `.deb` or `.dmg`, including dependency licences:

```sh
cargo install --locked --features cli cargo-about --version 0.9.0
direnv exec . python3 tools/release/package.py package
```

Debian packages must be built on a native Linux host, not in the Nix shell.

### Windows

The Windows build uses Rust's x64 MSVC toolchain. Install Visual Studio Build
Tools with the C++ workload and Windows SDK, rustup, Python 3.12, Git for
Windows, PowerShell 7, direnv 2.37.1 and MSYS2. Keep Git's Bash and direnv on
`PATH`.

In an MSYS2 MINGW64 shell, install the shader compiler's build tools:

```sh
pacman -S --needed git make patch python autoconf automake libtool flex bison \
    perl-JSON mingw-w64-x86_64-gcc mingw-w64-x86_64-cmake \
    mingw-w64-x86_64-ninja mingw-w64-x86_64-tools \
    mingw-w64-x86_64-vulkan-headers mingw-w64-x86_64-spirv-headers
```

Still in that shell, from the repository root:

```sh
export OPENZT2_NATIVE_DEPENDENCY_DIR="$PWD/build/native"
bash tools/release/build-native.sh
```

Then open PowerShell 7 with the Visual Studio x64 developer environment and
return to the repository root:

```powershell
rustup toolchain install 1.96.0-x86_64-pc-windows-msvc --profile minimal --component rustfmt,clippy
rustup override set 1.96.0-x86_64-pc-windows-msvc
$env:OPENZT2_RELEASE_HOST = '1'
$env:OPENZT2_NATIVE_DEPENDENCY_DIR = "$PWD/build/native"
$env:KAI_CARGO_TARGET_ROOT = "$env:LOCALAPPDATA/cargo-targets"
$env:PATH = "C:/msys64/mingw64/bin;$env:PATH"
direnv allow .
if ($LASTEXITCODE -ne 0) { throw 'direnv allow failed' }
$changes = direnv export json | ConvertFrom-Json -AsHashtable
if ($LASTEXITCODE -ne 0) { throw 'direnv export failed' }
foreach ($entry in $changes.GetEnumerator()) {
    [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, 'Process')
}
cargo build --locked -p openzt2_game --bin openzt2
```

Adjust the MSYS2 path if installed elsewhere. Windows uses `direnv export`
because `direnv exec` uses Unix executable lookup. After activation, run Cargo
commands directly in that PowerShell session.

For the release ZIP:

```powershell
cargo install --locked --features cli cargo-about --version 0.9.0
python tools/release/package.py package
```

### Release packages

Packages and `SHA256SUMS` go under `build/releases/`. The
[release workflow](.github/workflows/release-builds.yml) builds Windows,
macOS and Linux on a version tag or manual dispatch. Tag pushes publish a
GitHub release after all three packages pass checksum verification; manual
runs only upload workflow artifacts.

The tag must match the workspace version, prefixed with `v`. Versions
containing a hyphen, including alpha versions, publish as prereleases.

## Style guide

This isn't standard Rust style. A lot of it is “good code according to Kieran”
dogma, but I really think it's worth it. 🤷 These are the conventions we use
here, and contributions should follow them.

### Naming

We use long, specific names. A reader should be able to tell what a function
works on and what it does without opening its implementation.

```rust
// Too vague:
configure_content(&mut game_application, enabled_archive_paths)?;
shaders::prepare(bytecode, binding_layout)?;

// Preferred:
add_enabled_z2f_archives_as_default_asset_source(
    &mut game_application,
    enabled_archive_paths,
)?;
d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
    bytecode,
    binding_layout,
)?;
```

The same goes for types: `AnimalAnimationStateGraphsAndClips` rather than
`Animals`. Useful detail belongs in the name; extra words that say nothing
don't. Modules need a specific subject too. We don't use catch-all names such
as `common`, `core`, `utils`, `backend`, or `contracts`.

### Data and comments

Our preferred layout is much like an old C header: structs together in type
modules, with the procedures that work on them in separate modules. Related
fields sit together. Types, names and trailing comments line up where the
language's formatter allows it:

```c
typedef struct FontGlyphAtlas {
    int32_t     base_size;      // Font size used to generate the atlas
    int32_t     glyph_count;
    int32_t     glyph_padding;  // Pixels between glyphs in the atlas
    Texture2D   texture;
    Rectangle*  rectangles;     // One texture rectangle per glyph
    GlyphInfo*  glyphs;
} FontGlyphAtlas;
```

Rust uses standard `rustfmt` formatting, without exceptions to force alignment.

Comments are for things the code doesn't tell us: units, constraints, or why
an apparently odd decision is necessary. Most only need a line or two. We
don't put paragraphs of doc-comments above straightforward functions or
narrate each statement. A longer explanation is fine when there's something
that actually needs explaining.

### Ownership

Bevy owns the live game through entities, components, resources, assets,
messages and systems. Other code can hold handles, indexes and presentation
state, but mustn't keep a second game or UI world in sync with it.

Each piece of application state has one owner and one representation. Its
mutable data stays private, with operations for the callers that need access.
New types are fine when they have a job of their own. Copies of existing
records, duplicate registries and view models of the same data aren't.

It's easy to accidentally do otherwise when translating functionality from
the original game's XML-driven class hierarchy. Its mode classes combine
responsibilities such as UI handling, camera control and cursor selection.
For example, `ZTGameMode` includes save-wait cursor state. Those responsibilities
have their own systems in OpenZT2, so look at the rest of the architecture
before adding another owner.

For example, persistence tracks outstanding save operations and sends
`SetWaitCursor` when its busy state changes. The UI keeps that request in its
`WaitCursor` resource, and `present_cursor` updates Bevy's `CursorIcon` on the
primary window. Map loading doesn't need another request: the same system
reads `GamePhase::Loading` directly. Use a message to request an operation;
read existing state when that's all the presentation needs. Don't keep a
second copy inside the mode you're working on.

### Modules

Each public item has one import path, through its owning module. There are no
convenience re-exports, facades or preludes. Visibility is private by default,
`pub(crate)` for crate-wide APIs, and `pub` for external APIs.

Data definitions, persistence, queries, mutations and presentation belong in
separate modules. Execution files should be small. Data definitions can be
longer when keeping the related fields together makes them easier to read.
Constructors and getters that merely repeat a record's fields aren't needed;
methods should do some work or enforce a constraint.

Rust modules use `name/mod.rs`, including leaf modules. Cargo entry points keep
their required filenames (`lib.rs`, `main.rs`, build scripts, examples and
integration-test roots).

Executables contain argument handling and platform setup. Reusable code belongs
in the library. The entry point calls that code directly:

```rust
fn main() -> std::io::Result<()> {
    openzt2_game::run_game_application_from_process_arguments()
}
```

There shouldn't be a chain of forwarding functions between this call and the
work.

### Checks and commits

From the repository root:

```sh
direnv exec . cargo fmt --all --check
direnv exec . cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Lint settings belong in the workspace manifest and are inherited by every
crate. Clippy's `all`, `pedantic`, `nursery` and `cargo` groups are enabled.
First-party compiler and Clippy warnings are errors. `unsafe_code`,
`unreachable_pub` and `unused_must_use` are denied. Exceptions must be local
and explained; necessary unsafe code stays within a small, reviewed boundary.

Checks run after a coherent batch of changes. Tests should check behaviour,
not repeat the implementation. Lint rules aren't weakened to get a pass.

Commit subjects are short, lowercase descriptions of the change. Local work
is squashed before a public push; ask about the boundary if it isn't clear.

## AI use and submitted material

AI use must be disclosed with the contribution. Contributors must confirm that
a person has reviewed all code they are responsible for, fixed any problems
found, and takes responsibility for the result.

AI-generated art must be explicitly labelled, including images, textures,
models, audio and video. Our organisation's governance is still in development.
AI-generated art is more likely to be excluded than accepted, so contributors
shouldn't assume that submitting it means it will be used.

Do not submit original game assets or code, or other third-party copyrighted
or trademarked material.
