//! The edition rustfmt formats a file at.

use std::path::Path;

/// Editions `rustfmt` understands; anything else (a typo, a future edition) falls
/// back to [`DEFAULT_EDITION`] rather than making rustfmt error.
const KNOWN_EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];

/// Edition assumed when no `Cargo.toml` governs the file (e.g. a standalone
/// snippet). The newest edition is the most permissive about modern syntax.
const DEFAULT_EDITION: &str = "2024";

/// Resolve the Rust edition that applies to `file_path` for formatting, always
/// returning one of [`KNOWN_EDITIONS`]. Walks up to the nearest `Cargo.toml`,
/// honoring workspace inheritance and Cargo's "no `edition` key means 2015" rule,
/// and falls back to [`DEFAULT_EDITION`] when nothing declares one.
pub(super) fn edition(file_path: &Path) -> &'static str {
    resolve_edition(file_path)
        .as_deref()
        .and_then(clamp_edition)
        .unwrap_or(DEFAULT_EDITION)
}

fn clamp_edition(edition: &str) -> Option<&'static str> {
    KNOWN_EDITIONS
        .iter()
        .copied()
        .find(|known| *known == edition)
}

/// What a single manifest tells us about the edition.
#[derive(Debug, PartialEq, Eq)]
enum ManifestEdition {
    /// Determined by this manifest (explicit, defaulted to 2015, or inheritance
    /// satisfied by this same file's `[workspace.package]`).
    Edition(String),
    /// `[package] edition.workspace = true` but no `[workspace.package].edition`
    /// here — the value lives in a workspace manifest further up.
    InheritFromAbove,
    /// No `[package]` table (a virtual workspace manifest); keep walking.
    Unknown,
}

fn resolve_edition(file_path: &Path) -> Option<String> {
    let mut dir = file_path.parent();
    let mut inheriting = false;

    while let Some(current) = dir {
        if let Ok(text) = std::fs::read_to_string(current.join("Cargo.toml")) {
            if inheriting {
                if let Some(edition) = workspace_package_edition(&text) {
                    return Some(edition);
                }
            } else {
                match edition_in_manifest(&text) {
                    ManifestEdition::Edition(edition) => return Some(edition),
                    ManifestEdition::InheritFromAbove => inheriting = true,
                    ManifestEdition::Unknown => {}
                }
            }
        }
        dir = current.parent();
    }

    None
}

/// Extract the `[workspace.package].edition` from a manifest, if present.
fn workspace_package_edition(text: &str) -> Option<String> {
    let dom = taplo::parser::parse(text).into_dom();
    dom.get("workspace")
        .get("package")
        .get("edition")
        .as_str()
        .map(|s| s.value().to_string())
}

/// Interpret a single manifest's `[package]` edition, applying Cargo's rules.
fn edition_in_manifest(text: &str) -> ManifestEdition {
    let dom = taplo::parser::parse(text).into_dom();
    let package = dom.get("package");

    if package.as_table().is_none() {
        return ManifestEdition::Unknown;
    }

    let edition = package.get("edition");

    if let Some(value) = edition.as_str() {
        return ManifestEdition::Edition(value.value().to_string());
    }

    // `edition.workspace = true` / `edition = { workspace = true }`
    if edition
        .get("workspace")
        .as_bool()
        .is_some_and(|b| b.value())
    {
        return match workspace_package_edition(text) {
            Some(edition) => ManifestEdition::Edition(edition),
            None => ManifestEdition::InheritFromAbove,
        };
    }

    // `[package]` present without an `edition` key → Cargo defaults to 2015.
    ManifestEdition::Edition("2015".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn explicit_edition() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\nedition = \"2018\"\n"),
            ManifestEdition::Edition("2018".into())
        );
    }

    #[test]
    fn missing_edition_defaults_to_2015() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\n"),
            ManifestEdition::Edition("2015".into())
        );
    }

    #[test]
    fn workspace_inheritance_satisfied_in_same_file() {
        let manifest = "[package]\nname = \"x\"\nedition.workspace = true\n\n\
                        [workspace.package]\nedition = \"2021\"\n";
        assert_eq!(
            edition_in_manifest(manifest),
            ManifestEdition::Edition("2021".into())
        );
    }

    #[test]
    fn workspace_inheritance_deferred_upwards() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\nedition = { workspace = true }\n"),
            ManifestEdition::InheritFromAbove
        );
    }

    #[test]
    fn virtual_manifest_is_unknown() {
        assert_eq!(
            edition_in_manifest("[workspace]\nmembers = []\n"),
            ManifestEdition::Unknown
        );
    }

    #[test]
    fn clamp_rejects_unknown_editions() {
        assert_eq!(clamp_edition("2021"), Some("2021"));
        assert_eq!(clamp_edition("2030"), None);
        assert_eq!(clamp_edition(""), None);
    }

    #[test]
    fn walk_resolves_real_manifest() {
        // This crate's Cargo.toml declares edition 2024; a file inside it resolves
        // to that (and this is why the in-repo snapshot fixtures stay on 2024).
        let in_repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/formatting/rustfmt.rs");
        assert_eq!(edition(&in_repo), "2024");
    }

    #[test]
    fn walk_falls_back_when_no_manifest() {
        // No Cargo.toml anywhere above this path → default, no panic.
        assert_eq!(
            edition(Path::new("/semantic-edit-nonexistent/a/b/file.rs")),
            DEFAULT_EDITION
        );
    }
}
