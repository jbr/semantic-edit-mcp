//! Finding a formatter's program the way the platform's shell finds one: on the
//! `PATH`, and on Windows under each of `PATHEXT`'s extensions, so that npm's
//! `.cmd` shims are found.
//!
//! The search itself ([`Search::find`]) takes the `PATH` directories, the
//! extensions and the file check as values, so the Windows rules are tested on
//! every platform.

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

/// The extensions Windows tries when `PATHEXT` is unset or empty.
const DEFAULT_PATHEXT: &[&str] = &[".COM", ".EXE", ".BAT", ".CMD"];

/// The file `program` runs from, or why there is none.
///
/// A program named with a directory, like `./fmt` or `/usr/bin/rustfmt`, is
/// that file, relative to this process's working directory; a bare name is
/// looked up on `PATH`. Either way, Windows also tries each of `PATHEXT`'s
/// extensions. An empty `PATH` entry is skipped rather than read as the working
/// directory.
pub(super) fn resolve(program: &str) -> Result<PathBuf, String> {
    let search = Search::from_env();
    if let Some(path) = search.find(program, is_executable) {
        return Ok(path);
    }
    if !has_directory(program) {
        return Err(String::from("not found on PATH"));
    }
    // Named by its path: say what's wrong with it.
    let path = search.absolute(Path::new(program));
    Err(if path.is_file() {
        String::from("not executable")
    } else {
        String::from("no such file")
    })
}

/// Where a program is looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Search {
    /// The `PATH` directories, in order.
    dirs: Vec<PathBuf>,
    /// On Windows, the extensions tried for a name without one of them, in
    /// order; `None` elsewhere.
    extensions: Option<Vec<OsString>>,
    /// The directory a relative path with a directory is taken from.
    cwd: Option<PathBuf>,
}

impl Search {
    /// The search this process's environment describes.
    fn from_env() -> Self {
        let dirs = env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect())
            .unwrap_or_default();
        let extensions = cfg!(windows).then(|| extensions(env::var_os("PATHEXT").as_deref()));
        Self {
            dirs,
            extensions,
            cwd: env::current_dir().ok(),
        }
    }

    /// The first of `program`'s candidate files that `is_executable` accepts.
    fn find(&self, program: &str, is_executable: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        if program.is_empty() {
            return None;
        }
        if has_directory(program) {
            return self
                .candidates(&self.absolute(Path::new(program)))
                .into_iter()
                .find(|candidate| is_executable(candidate));
        }
        self.dirs
            .iter()
            .filter(|dir| !dir.as_os_str().is_empty())
            .flat_map(|dir| self.candidates(&dir.join(program)))
            .find(|candidate| is_executable(candidate))
    }

    /// `path`, taken from the working directory if it is relative.
    fn absolute(&self, path: &Path) -> PathBuf {
        match &self.cwd {
            Some(cwd) if path.is_relative() => cwd.join(path),
            _ => path.to_path_buf(),
        }
    }

    /// The files `path` may name. On Windows that is `path` itself when it
    /// already ends in one of the extensions. Otherwise it is `path` with each
    /// extension appended, after `path` itself if it has an extension of its
    /// own: an extensionless file, like the shell script npm puts beside its
    /// `.cmd` shim, isn't something Windows can run. Elsewhere it is `path`.
    fn candidates(&self, path: &Path) -> Vec<PathBuf> {
        let Some(extensions) = &self.extensions else {
            return vec![path.to_path_buf()];
        };
        if has_extension_in(path, extensions) {
            return vec![path.to_path_buf()];
        }
        let bare = path.extension().map(|_| path.to_path_buf());
        bare.into_iter()
            .chain(extensions.iter().map(|extension| {
                let mut candidate = path.as_os_str().to_os_string();
                candidate.push(extension);
                PathBuf::from(candidate)
            }))
            .collect()
    }
}

/// `PATHEXT`'s extensions as Windows reads them: `;`-separated, each beginning
/// with a dot, or [`DEFAULT_PATHEXT`] when there are none.
fn extensions(pathext: Option<&OsStr>) -> Vec<OsString> {
    let listed = pathext
        .map(|pathext| {
            pathext
                .to_string_lossy()
                .split(';')
                .map(str::trim)
                .filter(|extension| extension.len() > 1 && extension.starts_with('.'))
                .map(OsString::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if listed.is_empty() {
        DEFAULT_PATHEXT.iter().map(OsString::from).collect()
    } else {
        listed
    }
}

/// Whether `path` ends in one of `extensions`, which Windows compares without
/// regard to case.
fn has_extension_in(path: &Path, extensions: &[OsString]) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    let extension = extension.to_string_lossy();
    extensions.iter().any(|listed| {
        listed
            .to_string_lossy()
            .strip_prefix('.')
            .is_some_and(|listed| listed.eq_ignore_ascii_case(&extension))
    })
}

/// Whether `program` names a file by its path, rather than a name to look up.
fn has_directory(program: &str) -> bool {
    let path = Path::new(program);
    path.is_absolute() || path.components().count() > 1
}

/// Whether `path` is a file this process could run.
fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn windows(dirs: &[&str], pathext: Option<&str>) -> Search {
        Search {
            dirs: dirs.iter().map(PathBuf::from).collect(),
            extensions: Some(extensions(pathext.map(OsStr::new))),
            cwd: Some(PathBuf::from("/work")),
        }
    }

    fn unix(dirs: &[&str]) -> Search {
        Search {
            dirs: dirs.iter().map(PathBuf::from).collect(),
            extensions: None,
            cwd: Some(PathBuf::from("/work")),
        }
    }

    /// `search`'s choice for `program` among `files`, the only files there are.
    fn find(search: &Search, program: &str, files: &[&str]) -> Option<String> {
        let files = files.iter().map(PathBuf::from).collect::<BTreeSet<_>>();
        search
            .find(program, |path| files.contains(path))
            .map(|path| path.to_string_lossy().into_owned())
    }

    const PATHEXT: Option<&str> = Some(".COM;.EXE;.BAT;.CMD;.VBS;.JS");

    /// npm installs a program as an extensionless shell script, a `.cmd` shim
    /// and a PowerShell script. Windows can run only the shim.
    #[test]
    fn windows_finds_npms_cmd_shim() {
        let npm = ["/npm/biome", "/npm/biome.ps1", "/npm/biome.CMD"];
        assert_eq!(
            find(&windows(&["/npm"], PATHEXT), "biome", &npm).as_deref(),
            Some("/npm/biome.CMD")
        );
    }

    #[test]
    fn windows_tries_extensions_in_pathext_order_within_each_directory() {
        let search = windows(&["/first", "/second"], PATHEXT);
        assert_eq!(
            find(&search, "fmt", &["/first/fmt.CMD", "/first/fmt.EXE"]).as_deref(),
            Some("/first/fmt.EXE")
        );
        // An earlier directory wins over a preferred extension in a later one.
        assert_eq!(
            find(&search, "fmt", &["/first/fmt.CMD", "/second/fmt.EXE"]).as_deref(),
            Some("/first/fmt.CMD")
        );
    }

    #[test]
    fn windows_takes_a_name_with_a_listed_extension_as_is() {
        let search = windows(&["/npm"], PATHEXT);
        assert_eq!(
            find(&search, "biome.cmd", &["/npm/biome.cmd"]).as_deref(),
            Some("/npm/biome.cmd")
        );
        assert_eq!(find(&search, "biome.cmd", &["/npm/biome.cmd.EXE"]), None);
    }

    /// A name whose extension isn't listed may still be the file itself, or
    /// one with a listed extension after it.
    #[test]
    fn windows_tries_a_name_with_another_extension_bare_then_extended() {
        let search = windows(&["/bin"], PATHEXT);
        assert_eq!(
            find(&search, "fmt.py", &["/bin/fmt.py", "/bin/fmt.py.EXE"]).as_deref(),
            Some("/bin/fmt.py")
        );
        assert_eq!(
            find(&search, "fmt.py", &["/bin/fmt.py.EXE"]).as_deref(),
            Some("/bin/fmt.py.EXE")
        );
    }

    #[test]
    fn windows_matches_extensions_without_regard_to_case() {
        assert!(has_extension_in(
            Path::new("/npm/biome.cmd"),
            &extensions(Some(OsStr::new(".EXE;.CMD")))
        ));
        assert!(has_extension_in(
            Path::new("/npm/biome.Cmd"),
            &extensions(Some(OsStr::new(".exe;.cmd")))
        ));
        assert!(!has_extension_in(
            Path::new("/npm/biome.ps1"),
            &extensions(Some(OsStr::new(".EXE;.CMD")))
        ));
        assert!(!has_extension_in(
            Path::new("/npm/biome"),
            &extensions(None)
        ));
    }

    #[test]
    fn pathext_defaults_when_unset_or_empty_and_skips_malformed_entries() {
        let default = DEFAULT_PATHEXT
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        assert_eq!(extensions(None), default);
        assert_eq!(extensions(Some(OsStr::new(""))), default);
        assert_eq!(extensions(Some(OsStr::new(";;"))), default);
        assert_eq!(
            extensions(Some(OsStr::new(".EXE; .CMD ;BAT;.;"))),
            [".EXE", ".CMD"].map(OsString::from)
        );
    }

    #[test]
    fn windows_applies_pathext_to_a_path_with_a_directory() {
        let search = windows(&["/npm"], PATHEXT);
        assert_eq!(
            find(
                &search,
                "node_modules/.bin/prettier",
                &[
                    "/work/node_modules/.bin/prettier",
                    "/work/node_modules/.bin/prettier.CMD"
                ]
            )
            .as_deref(),
            Some("/work/node_modules/.bin/prettier.CMD")
        );
    }

    #[test]
    fn unix_takes_the_first_match_on_path_and_no_extensions() {
        let search = unix(&["/usr/local/bin", "/usr/bin"]);
        assert_eq!(
            find(
                &search,
                "rustfmt",
                &["/usr/bin/rustfmt", "/usr/local/bin/rustfmt"]
            )
            .as_deref(),
            Some("/usr/local/bin/rustfmt")
        );
        assert_eq!(find(&search, "biome", &["/usr/bin/biome.cmd"]), None);
    }

    /// An empty `PATH` entry would otherwise mean the working directory, whose
    /// files are no more trusted than any project's.
    #[test]
    fn an_empty_path_entry_is_not_the_working_directory() {
        let search = unix(&["", "/usr/bin"]);
        assert_eq!(find(&search, "fmt", &["fmt", "/work/fmt"]), None);
        assert_eq!(
            find(&search, "fmt", &["fmt", "/usr/bin/fmt"]).as_deref(),
            Some("/usr/bin/fmt")
        );
    }

    #[test]
    fn a_path_with_a_directory_is_taken_from_the_working_directory_not_path() {
        let search = unix(&["/usr/bin"]);
        assert_eq!(
            find(&search, "./fmt", &["/usr/bin/fmt", "/work/./fmt"]).as_deref(),
            Some("/work/./fmt")
        );
        assert_eq!(
            find(&search, "bin/fmt", &["/work/bin/fmt"]).as_deref(),
            Some("/work/bin/fmt")
        );
        assert_eq!(
            find(&search, "/opt/fmt", &["/opt/fmt"]).as_deref(),
            Some("/opt/fmt")
        );
        assert_eq!(find(&search, "./fmt", &["/usr/bin/fmt"]), None);
    }

    #[test]
    fn an_empty_name_is_nothing() {
        assert_eq!(find(&unix(&["/usr/bin"]), "", &["/usr/bin"]), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_real_program_resolves_on_this_platform() {
        let path = resolve("sh").unwrap();
        assert!(path.is_absolute(), "{}", path.display());
        assert!(is_executable(&path), "{}", path.display());
    }

    #[test]
    fn a_missing_program_says_why() {
        assert_eq!(
            resolve("semantic-edit-no-such-program").unwrap_err(),
            "not found on PATH"
        );
        assert_eq!(
            resolve("./semantic-edit-no-such-program").unwrap_err(),
            "no such file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_permission_to_run_says_so() {
        let dir =
            std::env::temp_dir().join(format!("semantic-edit-resolve-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fmt");
        fs::write(&file, "#!/bin/sh\ncat\n").unwrap();
        let result = resolve(file.to_str().unwrap());
        fs::remove_dir_all(&dir).ok();
        assert_eq!(result.unwrap_err(), "not executable");
    }
}
