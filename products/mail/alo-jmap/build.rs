use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ALO_BUILD_REVISION");
    println!("cargo:rerun-if-changed=../../../platform/alo-store/migrations");

    let revision = std::env::var("ALO_BUILD_REVISION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_revision)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=ALO_BUILD_REVISION={revision}");

    let schema = latest_schema(Path::new("../../../platform/alo-store/migrations"));
    println!("cargo:rustc-env=ALO_BUILD_SCHEMA={schema}");
}

fn git_revision() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn latest_schema(directory: &Path) -> u64 {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.split_once('_')?.0.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}
