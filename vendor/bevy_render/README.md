# Bevy render 0.19.1

Source: crates.io `bevy_render` 0.19.1, retaining its MIT and Apache licences.

Local change in `src/storage.rs`: changed `ShaderBuffer` data reuses wgpu's
`StagingBelt` instead of allocating staging memory through one queue write per
buffer. Copies are submitted after shader-buffer preparation and before the
render stages. Recall occurs after submission, and render-device recovery
reinitializes the staging resource. Initial allocation and resize semantics
remain Bevy-owned. Tracy records uploaded buffer and byte counts.

Opted-in fixed-layout shader uniforms now use aligned ranges in shared 1 MiB
GPU pages. `offset-allocator` owns range allocation and reclamation. Bevy Buffer
clones retain each range until its asset and prepared bindings release it;
empty pages have no strong owner in the allocator. Logical Buffer IDs remain
distinct for binding caches, while wgpu sees only the backing page buffers.
Binding offsets and sizes preserve the existing shader ABI. Source assets and
their dependency handles remain unchanged; no CPU register mirror is added.

Focused ownership test, from the reimplementation workspace's direnv:
`cargo test --manifest-path vendor/bevy_render/Cargo.toml --release --lib uniform_buffer_pages::tests`.
The fork is excluded from first-party workspace membership so its upstream
test dependencies do not enter the game's dependency resolution.
