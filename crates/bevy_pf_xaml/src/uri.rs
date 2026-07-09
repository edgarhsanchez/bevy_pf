//! WPF resource URI parsing (`ResourceDictionary Source=`, pack URIs).
//!
//! WPF accepts five spellings that all reduce to (assembly, path):
//!
//! 1. `Styles.xaml` — relative to the referencing file
//! 2. `/Themes/Styles.xaml` — root-relative (application root)
//! 3. `/MyLib;component/Themes/Styles.xaml` — another assembly's resources
//! 4. `pack://application:,,,/Themes/Styles.xaml` (optionally with
//!    `/MyLib;component/...` after the authority)
//! 5. `pack://siteoforigin:,,,/...` — site-of-origin (treated like the
//!    application root here)

use crate::error::{XamlError, XamlResult};

/// A canonical resource location: optional assembly + `/`-separated path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PfUri {
    /// `Some` for `;component` references into another assembly/package.
    pub assembly: Option<String>,
    /// Normalized path, `/` separators, no leading slash.
    pub path: String,
    /// True when the path is relative to the application root rather than
    /// the referencing file.
    pub root_relative: bool,
}

impl PfUri {
    /// Parse any of the accepted spellings.
    pub fn parse(input: &str) -> XamlResult<PfUri> {
        let mut rest = input.trim().replace('\\', "/");
        if rest.is_empty() {
            return Err(XamlError::convert(input, "Uri", "empty uri"));
        }

        // pack://authority:,,,<path>
        let mut root_relative = false;
        if let Some(after_scheme) = rest.strip_prefix("pack://") {
            let Some(comma_end) = after_scheme.find(":,,,") else {
                return Err(XamlError::convert(
                    input,
                    "Uri",
                    "pack uri missing `:,,,` authority terminator",
                ));
            };
            let authority = &after_scheme[..comma_end];
            if !matches!(authority, "application" | "siteoforigin") {
                return Err(XamlError::convert(
                    input,
                    "Uri",
                    "pack authority must be application or siteoforigin",
                ));
            }
            rest = after_scheme[comma_end + 4..].to_string();
            root_relative = true;
        }

        if let Some(stripped) = rest.strip_prefix('/') {
            rest = stripped.to_string();
            root_relative = true;
        }

        // Leading `<Assembly>;component/` segment.
        let mut assembly = None;
        if let Some(semi) = rest.find(';') {
            let (asm, tail) = rest.split_at(semi);
            if let Some(component_path) = tail
                .strip_prefix(";component/")
                .or_else(|| tail.strip_prefix(";Component/"))
            {
                assembly = Some(asm.to_string());
                rest = component_path.to_string();
                root_relative = true;
            } else {
                return Err(XamlError::convert(
                    input,
                    "Uri",
                    "`;` in uri must be part of `;component/`",
                ));
            }
        }

        if rest.is_empty() {
            return Err(XamlError::convert(input, "Uri", "empty path"));
        }

        // Note: `.`/`..` segments are preserved here — they can only be
        // collapsed after joining with the referencing file's directory
        // (`resolve`), otherwise `../colors.xaml` would lose its meaning.
        Ok(PfUri {
            assembly,
            path: rest,
            root_relative,
        })
    }

    /// Resolve this uri against the directory of the referencing file
    /// (`base` uses `/` separators; empty string = the root). Root-relative
    /// and assembly uris ignore the base.
    pub fn resolve(&self, base_dir: &str) -> String {
        if self.root_relative || self.assembly.is_some() || base_dir.is_empty() {
            normalize(&self.path)
        } else {
            normalize(&format!("{base_dir}/{}", self.path))
        }
    }
}

/// Collapse `.` and `..` segments.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative() {
        let u = PfUri::parse("Styles.xaml").unwrap();
        assert_eq!(u.assembly, None);
        assert_eq!(u.path, "Styles.xaml");
        assert!(!u.root_relative);
        assert_eq!(u.resolve("Themes/Dark"), "Themes/Dark/Styles.xaml");
    }

    #[test]
    fn relative_with_parent_segments() {
        let u = PfUri::parse("../Shared/Colors.xaml").unwrap();
        assert_eq!(u.resolve("Themes/Dark"), "Themes/Shared/Colors.xaml");
    }

    #[test]
    fn root_relative() {
        let u = PfUri::parse("/Themes/Styles.xaml").unwrap();
        assert!(u.root_relative);
        assert_eq!(u.path, "Themes/Styles.xaml");
        assert_eq!(u.resolve("anywhere"), "Themes/Styles.xaml");
    }

    #[test]
    fn component_assembly() {
        let u = PfUri::parse("/MyLib;component/Themes/Styles.xaml").unwrap();
        assert_eq!(u.assembly.as_deref(), Some("MyLib"));
        assert_eq!(u.path, "Themes/Styles.xaml");
        assert!(u.root_relative);
    }

    #[test]
    fn pack_application() {
        let u = PfUri::parse("pack://application:,,,/Themes/Styles.xaml").unwrap();
        assert_eq!(u.assembly, None);
        assert_eq!(u.path, "Themes/Styles.xaml");
        assert!(u.root_relative);

        let u =
            PfUri::parse("pack://application:,,,/MyLib;component/A/B.xaml").unwrap();
        assert_eq!(u.assembly.as_deref(), Some("MyLib"));
        assert_eq!(u.path, "A/B.xaml");
    }

    #[test]
    fn pack_siteoforigin_and_backslashes() {
        let u = PfUri::parse(r"pack://siteoforigin:,,,/Data\Styles.xaml").unwrap();
        assert_eq!(u.path, "Data/Styles.xaml");
    }

    #[test]
    fn errors() {
        assert!(PfUri::parse("").is_err());
        assert!(PfUri::parse("pack://application/Styles.xaml").is_err());
        assert!(PfUri::parse("/Lib;notcomponent/x.xaml").is_err());
    }
}
