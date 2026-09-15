use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn main() {
    // The handler remains the single command registry. Once an application
    // ACL exists, Tauri requires grants for business IPC as well as window IPC.
    // Generate four finite permission groups instead of duplicating that list.
    println!("cargo:rerun-if-changed=src/lib.rs");
    let source = fs::read_to_string("src/lib.rs").expect("read command registry");
    let registry = source
        .split_once("tauri::generate_handler![")
        .and_then(|(_, tail)| tail.split_once("])").map(|(body, _)| body))
        .expect("explicit Tauri command registry");
    let mut business = BTreeSet::new();
    let mut shell = BTreeSet::new();
    for line in registry.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("//") || line.starts_with("#[cfg(") {
            continue;
        }
        let command = line
            .strip_suffix(',')
            .expect("one command per registry line");
        let parts: Vec<_> = command.split("::").collect();
        assert!(
            parts.len() == 3 && parts[0] == "commands",
            "unexpected command path: {command}"
        );
        let name = parts[2];
        assert!(
            name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'),
            "invalid command: {name}"
        );
        let group = if parts[1] == "desktop" {
            &mut shell
        } else {
            &mut business
        };
        assert!(group.insert(name), "duplicate command: {name}");
    }
    assert!(
        !business.is_empty(),
        "business permissions must not be empty"
    );
    assert_eq!(
        shell,
        BTreeSet::from([
            "desktop_shell_state",
            "desktop_shell_ready",
            "desktop_shell_regions"
        ])
    );
    let quoted = |name: &str| format!("\"{name}\"");
    let mut permissions = vec![format!(
        r#"{{"identifier":"allow-planner-commands","description":"Registered Goal commands for the local main window","commands":{{"allow":[{}]}}}}"#,
        business
            .into_iter()
            .map(quoted)
            .collect::<Vec<_>>()
            .join(",")
    )];
    for command in shell {
        permissions.push(format!(
            r#"{{"identifier":"allow-{}","description":"Windows main window shell command","commands":{{"allow":["{}"]}}}}"#,
            command.replace('_', "-"), command
        ));
    }
    let file = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"))
        .join("desktop-permissions.json");
    fs::write(
        &file,
        format!(r#"{{"permission":[{}]}}"#, permissions.join(",")),
    )
    .expect("write finite application permissions");
    // AppManifest takes a static glob; the path lives for this build process.
    let pattern = Box::leak(file.to_string_lossy().replace('\\', "/").into_boxed_str());
    let manifest = tauri_build::AppManifest::new().permissions_path_pattern(pattern);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to prepare Tauri build");
}
