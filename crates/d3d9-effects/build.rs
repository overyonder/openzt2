use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=native/compiler.c");
    println!("cargo:rerun-if-changed=native/shader.c");
    println!("cargo:rerun-if-changed=native/effect_execution_validation.h");
    println!("cargo:rerun-if-env-changed=OPENZT2_NATIVE_DEPENDENCY_DIR");

    let compilation_target = std::env::var("TARGET")
        .unwrap_or_else(|error| panic!("Cargo must provide TARGET to build d3d9-effects: {error}"));

    if compilation_target.contains("windows") {
        compile_native_effect_wrappers_for_windows();
    } else {
        compile_native_effect_wrappers_with_project_flake_dependencies();
    }
}

fn compile_native_effect_wrappers_for_windows() {
    let native_dependency_directory = std::env::var_os("OPENZT2_NATIVE_DEPENDENCY_DIR")
        .map_or_else(|| {
            panic!(
                "OPENZT2_NATIVE_DEPENDENCY_DIR must name the extracted OpenZT2 Windows native dependency bundle"
            )
        }, PathBuf::from);
    let native_dependency_include_directory = native_dependency_directory.join("include");
    let native_dependency_library_directory = native_dependency_directory.join("lib");
    let native_dependency_runtime_directory = native_dependency_directory.join("bin");

    require_native_dependency_file(
        &native_dependency_include_directory.join("vkd3d/vkd3d_shader.h"),
    );
    require_native_dependency_file(
        &native_dependency_include_directory.join("vkd3d/vkd3d_types.h"),
    );
    require_native_dependency_file(&native_dependency_include_directory.join("mojoshader.h"));
    require_native_dependency_file(
        &native_dependency_include_directory.join("mojoshader_effects.h"),
    );
    require_native_dependency_file(&native_dependency_library_directory.join("vkd3d-shader.lib"));
    require_native_dependency_file(&native_dependency_library_directory.join("mojoshader.lib"));
    require_native_dependency_file(&native_dependency_runtime_directory.join("vkd3d-shader.dll"));
    require_native_dependency_file(&native_dependency_runtime_directory.join("mojoshader.dll"));

    cc::Build::new()
        .file("native/compiler.c")
        .include(&native_dependency_include_directory)
        .include(native_dependency_include_directory.join("vkd3d"))
        .warnings(true)
        .compile("openzt2_effect_compiler");
    cc::Build::new()
        .file("native/shader.c")
        .include(&native_dependency_include_directory)
        .define("DECLSPEC", Some("__declspec(dllimport)"))
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .define("MOJOSHADER_NO_VERSION_INCLUDE", None)
        .define("MOJOSHADER_EFFECT_SUPPORT", None)
        .warnings(true)
        .warnings_into_errors(true)
        .compile("openzt2_effect_shader");

    println!(
        "cargo:rustc-link-search=native={}",
        native_dependency_library_directory.display()
    );
    println!("cargo:rustc-link-lib=dylib=vkd3d-shader");
    println!("cargo:rustc-link-lib=dylib=mojoshader");

    copy_windows_native_dependency_libraries_beside_game_executable(
        &native_dependency_runtime_directory,
    );
}

fn copy_windows_native_dependency_libraries_beside_game_executable(
    native_dependency_runtime_directory: &Path,
) {
    let cargo_build_output_directory = std::env::var_os("OUT_DIR").map_or_else(
        || panic!("Cargo must provide OUT_DIR to build d3d9-effects"),
        PathBuf::from,
    );
    let cargo_profile_output_directory = cargo_build_output_directory
        .ancestors()
        .nth(3)
        .unwrap_or_else(|| {
            panic!(
                "Cargo OUT_DIR has an unexpected layout: {}",
                cargo_build_output_directory.display()
            )
        });

    for native_dependency_library_filename in ["vkd3d-shader.dll", "mojoshader.dll"] {
        let native_dependency_library_source =
            native_dependency_runtime_directory.join(native_dependency_library_filename);
        let native_dependency_library_destination =
            cargo_profile_output_directory.join(native_dependency_library_filename);
        // A previous copy from the Nix store retains its read-only mode.
        // Replace this generated file instead of trying to truncate it.
        match std::fs::remove_file(&native_dependency_library_destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!(
                "could not replace {}: {error}",
                native_dependency_library_destination.display()
            ),
        }
        std::fs::copy(
            &native_dependency_library_source,
            &native_dependency_library_destination,
        )
        .unwrap_or_else(|error| {
            panic!(
                "could not copy {} to {}: {error}",
                native_dependency_library_source.display(),
                native_dependency_library_destination.display()
            )
        });
    }
}

fn require_native_dependency_file(native_dependency_file: &Path) {
    assert!(
        native_dependency_file.is_file(),
        "the OpenZT2 Windows native dependency bundle is missing {}",
        native_dependency_file.display()
    );
}

fn compile_native_effect_wrappers_with_project_flake_dependencies() {
    let vkd3d_shader_library_configuration = pkg_config::Config::new()
        .statik(false)
        .probe("libvkd3d-shader")
        .unwrap_or_else(|error| panic!("the project flake must provide vkd3d-shader: {error}"));
    let mut vkd3d_effect_compiler_wrapper_build = cc::Build::new();
    vkd3d_effect_compiler_wrapper_build
        .file("native/compiler.c")
        .warnings(true);
    for vkd3d_shader_include_path in vkd3d_shader_library_configuration.include_paths {
        vkd3d_effect_compiler_wrapper_build.include(vkd3d_shader_include_path);
    }
    vkd3d_effect_compiler_wrapper_build.compile("openzt2_effect_compiler");

    let mojoshader_include_directory = std::env::var_os("MOJOSHADER_DIR").map_or_else(
        || panic!("the project flake must provide MOJOSHADER_DIR"),
        PathBuf::from,
    );
    cc::Build::new()
        .file("native/shader.c")
        .include(mojoshader_include_directory)
        .define("MOJOSHADER_NO_VERSION_INCLUDE", None)
        .define("MOJOSHADER_EFFECT_SUPPORT", None)
        .warnings(true)
        .warnings_into_errors(true)
        .compile("openzt2_effect_shader");
    println!("cargo:rustc-link-lib=mojoshader");
}
