use std::path::Path;
use std::process::Command;

fn main() {
    // Tell cargo to rerun this script if frontend source files change
    if Path::new("frontend/src").is_dir() {
        walkdir_rerun("frontend/src");
        for file in &[
            "frontend/index.html",
            "frontend/package.json",
            "frontend/package-lock.json",
            "frontend/vite.config.ts",
            "frontend/tsconfig.json",
        ] {
            if Path::new(file).exists() {
                println!("cargo:rerun-if-changed={file}");
            }
        }
    }

    let dist = Path::new("frontend/dist");
    let frontend = Path::new("frontend");

    // Build the frontend if dist/ doesn't exist
    if !dist.join("index.html").exists() && frontend.exists() {
        println!("cargo:rerun-if-changed=frontend/dist");

        // Check for node
        if !check_cmd("node") {
            println!("cargo:warning=node not found — skipping frontend build");
            return;
        }

        // npm install
        let install = Command::new("npm")
            .current_dir(frontend)
            .args(["install"])
            .status();
        match install {
            Ok(s) if s.success() => {}
            Ok(s) => {
                println!("cargo:warning=npm install failed with {}", s);
                return;
            }
            Err(e) => {
                println!("cargo:warning=npm install not found: {e}");
                return;
            }
        }

        // npm run build
        let build = Command::new("npm")
            .current_dir(frontend)
            .args(["run", "build"])
            .output();
        match build {
            Ok(out) if out.status.success() => {
                println!("cargo:warning=frontend built successfully");
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                println!("cargo:warning=frontend build failed: {err}");
            }
            Err(e) => {
                println!("cargo:warning=frontend build failed: {e}");
            }
        }
    }
}

fn check_cmd(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").status().is_ok_and(|s| s.success())
}

fn walkdir_rerun(dir: &str) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walkdir_rerun(&path.to_string_lossy());
            } else {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}
