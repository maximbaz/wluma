use std::process::Command;

fn main() {
    let version = match std::env::var("WLUMA_VERSION") {
        Ok(v) => v,
        Err(_) => {
            let version = "$Format:%(describe)$"; // Replaced by git-archive.
            let version = if version.starts_with('$') {
                match Command::new("git").args(["describe", "--tags"]).output() {
                    Ok(o) if o.status.success() => {
                        String::from_utf8_lossy(&o.stdout).trim().to_string()
                    }
                    Ok(o) => panic!("git-describe exited non-zero: {}", o.status),
                    Err(err) => panic!("failed to execute git-describe: {err}"),
                }
            } else {
                version.to_string()
            };

            let version = version.strip_prefix('v').unwrap_or(&version);
            println!("cargo:rustc-env=WLUMA_VERSION={version}");
            version.to_string()
        }
    };

    let parts = version
        .split(|c: char| !c.is_ascii_digit())
        .collect::<Vec<_>>();

    if parts.len() < 3 {
        panic!("Unable to parse 'major.minor.patch' from version: {version}");
    }

    println!("cargo:rustc-env=WLUMA_VERSION_MAJOR={}", parts[0]);
    println!("cargo:rustc-env=WLUMA_VERSION_MINOR={}", parts[1]);
    println!("cargo:rustc-env=WLUMA_VERSION_PATCH={}", parts[2]);
}
