//! The device the app is running on, and MAUI's `{OnPlatform}` / `{OnIdiom}`.
//!
//! Both extensions have the same shape as `{AppThemeBinding}` — named arms
//! plus a `Default` that is also the ContentProperty — but they differ from it
//! in one way that simplifies everything: the platform and the idiom do not
//! change while the app runs. So they resolve ONCE, at instantiation, and need
//! no refresh pass, no recorded references and no store entries of their own.
//! MAUI treats both as fixed for the process too.
//!
//! From `dotnet/maui` `OnPlatformExtension.cs` / `OnIdiomExtension.cs`: an arm
//! that is not written falls back to `Default`, and an extension where nothing
//! is written at all is an error.
//!
//! Platform names are MAUI's, so MAUI markup ports across unchanged. Two of
//! them cannot occur here (`Tizen`, and the obsolete `UWP`), and three are
//! .NET-only host names (`GTK`, `WPF`, `macOS`); they parse and simply never
//! match, which is better than rejecting XAML that is valid upstream. `Linux`
//! and `Web` are additions — bevy runs there and MAUI has no name for it.

use bevy::prelude::*;

/// The platform names `{OnPlatform}` accepts. MAUI's set, plus `Linux` and
/// `Web`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePlatform {
    Android,
    IOS,
    MacCatalyst,
    /// The native Mac target, as distinct from Mac Catalyst. A bevy app on a
    /// Mac is this; `MacCatalyst` is accepted as an alias so MAUI markup
    /// works, but when a document writes BOTH, `macOS` wins.
    MacOS,
    WinUI,
    Tizen,
    Linux,
    Web,
    /// A target none of the names above describe.
    Other,
}

/// The idiom names `{OnIdiom}` accepts — MAUI's set exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceIdiom {
    Phone,
    Tablet,
    Desktop,
    TV,
    Watch,
}

/// Which of the two selectors an extension is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorKind {
    Platform,
    Idiom,
}

impl SelectorKind {
    /// The arm names this selector accepts, so a typo can be reported with
    /// the alternatives rather than silently skipped.
    pub fn arm_names(self) -> &'static [&'static str] {
        match self {
            // MAUI's names first, then the two additions. `UWP`, `GTK` and
            // `WPF` are accepted-but-never-matched for portability.
            SelectorKind::Platform => &[
                "Android",
                "iOS",
                "MacCatalyst",
                "macOS",
                "WinUI",
                "Tizen",
                "UWP",
                "GTK",
                "WPF",
                "Linux",
                "Web",
            ],
            SelectorKind::Idiom => &["Phone", "Tablet", "Desktop", "TV", "Watch"],
        }
    }

    pub fn extension_name(self) -> &'static str {
        match self {
            SelectorKind::Platform => "OnPlatform",
            SelectorKind::Idiom => "OnIdiom",
        }
    }
}

impl DevicePlatform {
    /// The platform this binary was built for.
    pub const fn current() -> Self {
        if cfg!(target_os = "android") {
            DevicePlatform::Android
        } else if cfg!(target_os = "ios") {
            DevicePlatform::IOS
        } else if cfg!(target_os = "macos") {
            DevicePlatform::MacOS
        } else if cfg!(target_os = "windows") {
            DevicePlatform::WinUI
        } else if cfg!(target_family = "wasm") {
            DevicePlatform::Web
        } else if cfg!(target_os = "linux") {
            DevicePlatform::Linux
        } else {
            DevicePlatform::Other
        }
    }

    /// Does an arm written `name` describe this platform?
    ///
    /// `MacCatalyst` matches a Mac so MAUI markup works unchanged; when a
    /// document writes both `macOS` and `MacCatalyst`, the caller resolves
    /// `macOS` first.
    pub fn matches(self, name: &str) -> bool {
        match name {
            "Android" => self == DevicePlatform::Android,
            "iOS" => self == DevicePlatform::IOS,
            "macOS" | "MacCatalyst" => self == DevicePlatform::MacOS,
            "WinUI" => self == DevicePlatform::WinUI,
            "Tizen" => self == DevicePlatform::Tizen,
            "Linux" => self == DevicePlatform::Linux,
            "Web" => self == DevicePlatform::Web,
            // .NET host names with no analog here: they parse, so MAUI markup
            // is accepted, and never match.
            "UWP" | "GTK" | "WPF" => false,
            _ => false,
        }
    }
}

impl DeviceIdiom {
    /// The idiom implied by the build target.
    ///
    /// A phone and a tablet are the same build, and nothing here can tell
    /// them apart — MAUI gets that from the OS. Mobile therefore defaults to
    /// `Phone`; a host that knows better should say so via [`PfDevice`].
    pub const fn current() -> Self {
        if cfg!(target_os = "android") || cfg!(target_os = "ios") {
            DeviceIdiom::Phone
        } else {
            DeviceIdiom::Desktop
        }
    }

    pub fn matches(self, name: &str) -> bool {
        match name {
            "Phone" => self == DeviceIdiom::Phone,
            "Tablet" => self == DeviceIdiom::Tablet,
            "Desktop" => self == DeviceIdiom::Desktop,
            "TV" => self == DeviceIdiom::TV,
            "Watch" => self == DeviceIdiom::Watch,
            _ => false,
        }
    }
}

/// What `{OnPlatform}` and `{OnIdiom}` resolve against.
///
/// Set it before building the UI. Unlike the app theme, changing it later
/// does NOT re-resolve anything already built — the platform and idiom are
/// fixed for a run, so nothing watches them.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PfDevice {
    pub platform: DevicePlatform,
    pub idiom: DeviceIdiom,
}

impl Default for PfDevice {
    fn default() -> Self {
        PfDevice {
            platform: DevicePlatform::current(),
            idiom: DeviceIdiom::current(),
        }
    }
}

impl PfDevice {
    /// Does `name` describe this device, for the given selector?
    pub fn matches(&self, kind: SelectorKind, name: &str) -> bool {
        match kind {
            SelectorKind::Platform => self.platform.matches(name),
            SelectorKind::Idiom => self.idiom.matches(name),
        }
    }
}

/// The device to resolve against, defaulting to the build target when the
/// resource is absent (a `World` built without the plugin).
pub fn device(world: &World) -> PfDevice {
    world.get_resource::<PfDevice>().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_names_are_mauis_and_only_one_can_match() {
        let mac = DevicePlatform::MacOS;
        assert!(mac.matches("macOS"));
        assert!(mac.matches("MacCatalyst"), "MAUI markup must port unchanged");
        assert!(!mac.matches("Android"));
        assert!(!mac.matches("iOS"));
        assert!(!mac.matches("WinUI"));

        // Names that exist in MAUI but can never describe a bevy target
        // parse and never match, rather than being rejected.
        for host in ["UWP", "GTK", "WPF"] {
            for p in [
                DevicePlatform::Android,
                DevicePlatform::IOS,
                DevicePlatform::MacOS,
                DevicePlatform::WinUI,
                DevicePlatform::Linux,
                DevicePlatform::Web,
            ] {
                assert!(!p.matches(host), "{host} must never match {p:?}");
            }
        }
    }

    #[test]
    fn every_advertised_arm_name_is_understood() {
        // A name the parser accepts but `matches` does not know would be a
        // silent always-Default arm.
        for kind in [SelectorKind::Platform, SelectorKind::Idiom] {
            for name in kind.arm_names() {
                let recognised = match kind {
                    SelectorKind::Platform => [
                        DevicePlatform::Android,
                        DevicePlatform::IOS,
                        DevicePlatform::MacOS,
                        DevicePlatform::WinUI,
                        DevicePlatform::Tizen,
                        DevicePlatform::Linux,
                        DevicePlatform::Web,
                    ]
                    .iter()
                    .any(|p| p.matches(name)),
                    SelectorKind::Idiom => [
                        DeviceIdiom::Phone,
                        DeviceIdiom::Tablet,
                        DeviceIdiom::Desktop,
                        DeviceIdiom::TV,
                        DeviceIdiom::Watch,
                    ]
                    .iter()
                    .any(|i| i.matches(name)),
                };
                let host_only = matches!(*name, "UWP" | "GTK" | "WPF");
                assert!(
                    recognised || host_only,
                    "`{name}` is advertised by {} but matches nothing",
                    kind.extension_name()
                );
            }
        }
    }

    #[test]
    fn the_default_device_describes_this_build() {
        let device = PfDevice::default();
        assert_eq!(device.platform, DevicePlatform::current());
        // Whatever this test runs on, exactly one platform name matches.
        let hits = SelectorKind::Platform
            .arm_names()
            .iter()
            .filter(|n| device.platform.matches(n))
            .count();
        assert!(
            hits <= 2,
            "at most macOS + MacCatalyst may both match, got {hits}"
        );
    }
}
