//! The world an e2e case runs in: a temp forest, a registry row, a site
//! configuration, and a `PATH` of stubs (§AR-002-summons.1).
//!
//! Every case builds its own world and throws it away, so nothing here reads
//! the machine it runs on: the registry, the feed cache, the home directory and
//! everything a summons can reach are inside one temporary directory. That is
//! what makes a case a scenario rather than a test of this laptop — and it is
//! the same isolation §REQ-001-boundary.2 asks of ephor itself, which takes a
//! checkout and its own configuration and nothing else.
//!
//! The bindings are stubs, and deliberately so (§FS-001-forge-interface.2,
//! §FS-006-project-interface.3): a forge, a check verb, a gate and a runtime
//! are commands ephor summons, so a shell script standing in for one exercises
//! the whole seam without a forge, a CI system, or an agent runtime being
//! anywhere near the test.

// Each case is its own test binary and uses the part of this world it needs;
// the rest would read as dead code under `RUSTFLAGS=-D warnings`.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The project every case watches. One id, so a case reads as a story about a
/// project rather than as a fixture with names in it.
pub const PROJECT: &str = "demo";

pub struct World {
    dir: tempfile::TempDir,
}

impl World {
    /// A world with a forest on disk, an empty registry row for it, and a
    /// configuration that watches nothing yet. A case adds what it is about.
    pub fn new() -> World {
        // The world is built inside a base directory the operating system has
        // already resolved. macOS hands out `/var/folders/…` temporary
        // directories that are really `/private/var/…`, and a summoned shell
        // prints the second spelling as its `$PWD` — so a case asserting that
        // a runtime ran from the checkout compared two spellings of the same
        // directory and failed on macOS alone, which says nothing about the
        // behaviour it was written to pin down.
        let mut base = std::env::temp_dir();
        if !cfg!(windows) {
            if let Ok(resolved) = base.canonicalize() {
                base = resolved;
            }
        }
        let dir = tempfile::Builder::new()
            .tempdir_in(base)
            .expect("a temporary world");
        let world = World { dir };
        fs::create_dir_all(world.forest()).expect("the forest root");
        fs::create_dir_all(world.path().join("fakebin")).expect("a PATH of stubs");
        fs::write(
            world.path().join("agents.tmpl"),
            "# AGENTS.md\n\n{summary}\n",
        )
        .expect("the AGENTS template the registry schema asks for");
        world.register(json!({}));
        world.configure(json!({}));
        world
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The forest root: where the project is on disk, and the only thing a
    /// verb, a gate or a store is read from (§REQ-001-boundary.2).
    pub fn forest(&self) -> PathBuf {
        self.path().join(PROJECT)
    }

    pub fn registry_path(&self) -> PathBuf {
        self.path().join("workspaces.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.path().join("status.json")
    }

    /// Write the registry, with `row` merged over the minimum a project row
    /// needs. The row is where a person's authority lives: it says where the
    /// project is and how much of its manifest to believe
    /// (§FS-006-project-interface.2).
    pub fn register(&self, row: Value) {
        let mut project = json!({
            "id": PROJECT,
            "type": "monorepo",
            "display_name": "Demo",
            "root": self.forest().to_string_lossy(),
            "main_branch": "main",
            "branches": []
        });
        merge(&mut project, row);
        let registry = json!({
            "project_types": [{
                "id": "monorepo",
                "layout": "monorepo",
                "repos": [{
                    "id": "repo",
                    "path": ".",
                    "role": "Repository root",
                    "required": true,
                    "update_mode": "branch",
                    "default_branch": "{branch}"
                }],
                "agents": {
                    "template": self.path().join("agents.tmpl").to_string_lossy(),
                    "structure_intro": "This project uses a single repository root:",
                    "summary_template": "This project root tracks `{display_name}` on branch `{branch}`."
                }
            }],
            "hook_sets": [],
            "projects": [project]
        });
        write_json(&self.registry_path(), &registry);
    }

    /// Write the site configuration, with `config` merged over defaults that
    /// keep a case fast and offline.
    pub fn configure(&self, config: Value) {
        let mut settings = json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10 },
            "projects": { PROJECT: { "providers": [] } }
        });
        merge(&mut settings, config);
        write_json(&self.config_path(), &settings);
    }

    /// The registry as ephor reads it — for the parts of a scenario that ask
    /// the library a question the command line has no surface for.
    pub fn registry_doc(&self) -> Value {
        read_json(&self.registry_path())
    }

    /// An executable on the world's `PATH`. This is how a forge extension, a
    /// vendor CLI, or an agent runtime is installed for a case: ephor looks
    /// each of them up by name, so a name found first is the implementation.
    pub fn stub(&self, name: &str, body: &str) -> PathBuf {
        let path = self.path().join("fakebin").join(name);
        executable(&path, body);
        path
    }

    /// A file in the forest, named relative to its root.
    pub fn file(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.forest().join(relative);
        fs::create_dir_all(path.parent().expect("a parent directory")).expect("make the directory");
        fs::write(&path, body).expect("write the file");
        path
    }

    /// A runnable file in the forest — a check verb, a gate verb, or anything
    /// else the project binds to a path of its own.
    pub fn script(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.file(relative, body);
        set_executable(&path);
        path
    }

    /// The project's own manifest, at the forest root
    /// (§FS-006-project-interface.2).
    pub fn manifest(&self, manifest: Value) -> PathBuf {
        self.file(
            "ephor.json",
            &(serde_json::to_string_pretty(&manifest).expect("a manifest serializes") + "\n"),
        )
    }

    /// `ephor`, as a person runs it, with this world as its whole environment.
    pub fn ephor(&self) -> assert_cmd::Command {
        assert_cmd::Command::from_std(self.ephor_raw())
    }

    /// The same binary in the same world, as a plain command — for a case that
    /// has to point its streams somewhere, the way ephor points a job
    /// supervisor's at the job's log (§FS-005-dispatch.17).
    pub fn ephor_raw(&self) -> std::process::Command {
        let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("ephor"));
        command
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.path().join("fakebin").to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("HOME", self.path())
            .env("XDG_STATE_HOME", self.path().join("state"))
            .env("EPHOR_REGISTRY", self.registry_path())
            .env("EPHOR_STATUS_CONFIG", self.config_path());
        command
    }

    /// The cached feed, which is what a refresh left behind and every surface
    /// reads (§AR-008-pipeline.1).
    pub fn feed(&self) -> Value {
        read_json(
            &self
                .path()
                .join("state/ephor/feed")
                .join(format!("{PROJECT}.json")),
        )
    }

    /// Every matter in the cached feed, whichever source reported it.
    pub fn matters(&self) -> Vec<Value> {
        self.feed()["providers"]
            .as_object()
            .map(|providers| {
                providers
                    .values()
                    .filter_map(|slot| slot["matters"].as_array())
                    .flatten()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the feed holds a matter with this key at all — for a scenario
    /// whose point is that something is *not* there.
    pub fn has_matter(&self, key: &str) -> bool {
        self.matters()
            .iter()
            .any(|matter| matter["key"] == key || matter["id"] == key)
    }

    /// The matter with this key, or a panic naming what was there instead —
    /// a scenario that cannot find its subject has to say what it saw.
    pub fn matter(&self, key: &str) -> Value {
        let matters = self.matters();
        matters
            .iter()
            .find(|matter| matter["key"] == key || matter["id"] == key)
            .cloned()
            .unwrap_or_else(|| {
                let keys: Vec<&str> = matters
                    .iter()
                    .filter_map(|matter| matter["key"].as_str())
                    .collect();
                panic!("no matter {key} in the feed; it holds {keys:?}")
            })
    }

    pub fn read(&self, relative: &str) -> String {
        let path = self.path().join(relative);
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
    }
}

/// A shell script that writes an answer envelope and exits with `code`
/// (§FS-006-project-interface.3, §FS-006-project-interface.4). Every stub verb
/// in the cases is built from this: the contract is exit code plus the file
/// `$EPHOR_ANSWER` names, and nothing else.
pub fn answering(envelope: Value, code: i32) -> String {
    format!(
        "#!/usr/bin/env bash\nset -euo pipefail\ncat > \"$EPHOR_ANSWER\" <<'ENVELOPE'\n{}\nENVELOPE\nexit {code}\n",
        serde_json::to_string_pretty(&envelope).expect("an envelope serializes")
    )
}

pub fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("{} is not JSON: {err}", path.display()))
}

pub fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_string_pretty(value).expect("the fixture serializes"),
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

/// The JSON a command printed with `--json`, which is the shape a workflow or
/// another program reads.
pub fn json_of(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn executable(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("make the directory");
    fs::write(path, body).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    set_executable(path);
}

fn set_executable(path: &Path) {
    #[cfg(not(unix))]
    let _ = path;
    // The exec bit is a Unix thing, and so is the type that sets it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).expect("stat the script").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make it runnable");
    }
}

/// Deep-merge `overlay` into `base`, so a case states only the fields its
/// scenario is about.
fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}
