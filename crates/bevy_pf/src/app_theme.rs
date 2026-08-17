//! The app theme and MAUI's `{AppThemeBinding}` markup extension.
//!
//! MAUI semantics implemented here, from `dotnet/maui`
//! `src/Controls/src/Core/AppThemeBinding.cs` and
//! `AppThemeBindingExtension.cs`:
//!
//! - The requested theme is the USER's choice when they made one, and the
//!   platform's otherwise (`Application.cs`: `UserAppTheme` overrides
//!   `PlatformAppTheme`).
//! - `{AppThemeBinding Light=a, Dark=b, Default=c}` picks `Dark` under a dark
//!   theme and `Light` otherwise — `Unspecified` resolves as LIGHT, sharing
//!   the `_` arm of MAUI's switch rather than getting one of its own.
//! - A missing arm falls back to `Default` ONLY. Dark never falls back to
//!   Light, nor Light to Dark.
//! - "Supplied" means present in the markup, not non-null: `Light={x:Null}`
//!   IS supplied, so it yields null instead of falling through to `Default`.
//!   That is why an arm is `Option<ThemeArm>` and a null arm is
//!   `Some(ThemeArm::Value(None))`.
//! - When nothing matches and there is no `Default`, MAUI writes null to the
//!   target rather than reverting it. Here that is a `StoredValue` of `None`,
//!   which masks lower tiers — the same thing `{x:Null}` already does.
//! - Re-applying the theme already in effect does nothing (MAUI suppresses on
//!   `newTheme == _lastAppTheme`), so the generation counter below only moves
//!   when `requested()` actually changes.
//!
//! Where this stops short of MAUI is recorded in
//! `docs/maui-xaml-gap-analysis.md`; the short version is that live
//! re-resolution reaches only the store-managed properties, exactly the
//! ceiling `{DynamicResource}` already sits under.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window, WindowTheme, WindowThemeChanged};

use crate::provider::{PropertyTarget, StoredValue, ValueSource};
use crate::resources::ResourceKey;

/// MAUI's `AppTheme` (`AppTheme.shared.cs`), member for member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    /// No preference — resolves as [`AppTheme::Light`], per MAUI.
    #[default]
    Unspecified,
    Light,
    Dark,
}

impl AppTheme {
    fn from_window(theme: WindowTheme) -> Self {
        match theme {
            WindowTheme::Light => AppTheme::Light,
            WindowTheme::Dark => AppTheme::Dark,
        }
    }
}

/// The application's theme: what the host asked for, what the OS reports, and
/// a generation counter that moves only when the EFFECTIVE theme changes.
#[derive(Resource, Debug, Clone, Default)]
pub struct PfAppTheme {
    user: AppTheme,
    platform: AppTheme,
    generation: u64,
}

impl PfAppTheme {
    /// The theme in effect: the user's choice wins when they made one.
    pub fn requested(&self) -> AppTheme {
        if self.user != AppTheme::Unspecified {
            self.user
        } else {
            self.platform
        }
    }

    /// The host's explicit choice, if any.
    pub fn user(&self) -> AppTheme {
        self.user
    }

    /// What the OS reports, when the platform supports reporting it.
    pub fn platform(&self) -> AppTheme {
        self.platform
    }

    /// Bumped only when [`Self::requested`] changes value.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Set the host's choice. `Unspecified` hands control back to the OS.
    pub fn set_user(&mut self, theme: AppTheme) {
        let before = self.requested();
        self.user = theme;
        if self.requested() != before {
            self.generation += 1;
        }
    }

    pub(crate) fn set_platform(&mut self, theme: AppTheme) {
        let before = self.requested();
        self.platform = theme;
        if self.requested() != before {
            self.generation += 1;
        }
    }
}

/// The theme in effect, or [`AppTheme::Unspecified`] before the resource
/// exists (a headless `World` that never added the plugin).
pub fn app_theme(world: &World) -> AppTheme {
    world
        .get_resource::<PfAppTheme>()
        .map(PfAppTheme::requested)
        .unwrap_or_default()
}

/// Choose the app theme explicitly (MAUI's `Application.UserAppTheme`).
/// Passing [`AppTheme::Unspecified`] resumes following the OS.
pub fn set_user_app_theme(world: &mut World, theme: AppTheme) {
    let mut current = world.get_resource_or_insert_with(PfAppTheme::default);
    current.set_user(theme);
}

/// One arm of an `{AppThemeBinding}`, already converted for its property.
#[derive(Debug, Clone)]
pub enum ThemeArm {
    /// A value, or `None` for an arm written as `{x:Null}`.
    Value(StoredValue),
    /// `{DynamicResource key}` — re-resolved on every pick, so a theme flip
    /// and a dictionary swap both land.
    Dynamic(ResourceKey),
}

/// The Light/Dark/Default arms. `None` means the arm was NOT written; an arm
/// written as `{x:Null}` is `Some(ThemeArm::Value(None))` and stops the
/// fallback, which is what makes MAUI's "supplied, not non-null" rule work.
#[derive(Debug, Clone, Default)]
pub struct ThemeArms {
    pub light: Option<ThemeArm>,
    pub dark: Option<ThemeArm>,
    pub default: Option<ThemeArm>,
}

impl ThemeArms {
    /// MAUI's switch verbatim: Dark takes the Dark arm, EVERYTHING else
    /// (Light and Unspecified alike) takes the Light arm, and either falls
    /// back to Default and no further.
    pub fn pick(&self, theme: AppTheme) -> Option<&ThemeArm> {
        match theme {
            AppTheme::Dark => self.dark.as_ref().or(self.default.as_ref()),
            _ => self.light.as_ref().or(self.default.as_ref()),
        }
    }
}

/// One recorded `{AppThemeBinding}`, the sibling of
/// [`crate::dynamic::DynEntry`].
#[derive(Debug, Clone)]
pub struct AppThemeEntry {
    pub arms: ThemeArms,
    pub target: PropertyTarget,
    /// The tier this writes at, so a refresh can never clobber a higher one.
    pub tier: ValueSource,
}

/// All `{AppThemeBinding}` references on an entity.
#[derive(Component, Debug, Default, Clone)]
pub struct PfAppThemeRefs(pub Vec<AppThemeEntry>);

/// Tracks the last theme generation the refresh pass applied.
#[derive(Resource, Default)]
pub(crate) struct LastAppThemeGen(pub u64);

/// Follow the OS appearance into [`PfAppTheme::platform`].
///
/// Reads the primary window's reported theme and the change messages winit
/// sends. It deliberately never WRITES `Window::window_theme`: bevy pushes
/// that back to `winit_window.set_theme`, which pins the window's appearance
/// and stops further notifications arriving.
///
/// `Window::window_theme` is `None` on iOS, Android and web, so on those
/// platforms the platform theme stays `Unspecified` — resolving as Light —
/// until the host calls [`set_user_app_theme`].
pub(crate) fn track_os_theme(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut changes: MessageReader<WindowThemeChanged>,
    mut theme: ResMut<PfAppTheme>,
) {
    // A change message is the more current signal, so it is read last.
    if let Ok(window) = windows.single()
        && let Some(reported) = window.window_theme
    {
        theme.set_platform(AppTheme::from_window(reported));
    }
    for change in changes.read() {
        theme.set_platform(AppTheme::from_window(change.theme));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arms(light: Option<&str>, dark: Option<&str>, default: Option<&str>) -> ThemeArms {
        let arm = |s: Option<&str>| {
            s.map(|s| match s {
                "null" => ThemeArm::Value(None),
                other => ThemeArm::Value(Some(crate::resources::PfValue::String(other.into()))),
            })
        };
        ThemeArms {
            light: arm(light),
            dark: arm(dark),
            default: arm(default),
        }
    }

    fn picked(arms: &ThemeArms, theme: AppTheme) -> Option<Option<String>> {
        arms.pick(theme).map(|a| match a {
            ThemeArm::Value(Some(crate::resources::PfValue::String(s))) => Some(s.clone()),
            ThemeArm::Value(_) => None,
            ThemeArm::Dynamic(_) => Some("<dynamic>".into()),
        })
    }

    #[test]
    fn unspecified_resolves_as_light_not_as_default() {
        let a = arms(Some("day"), Some("night"), Some("fallback"));
        assert_eq!(picked(&a, AppTheme::Light), Some(Some("day".into())));
        assert_eq!(picked(&a, AppTheme::Dark), Some(Some("night".into())));
        assert_eq!(
            picked(&a, AppTheme::Unspecified),
            Some(Some("day".into())),
            "Unspecified shares Light's arm — it is NOT its own case"
        );
    }

    #[test]
    fn a_missing_arm_falls_back_to_default_and_never_to_the_other_theme() {
        let no_dark = arms(Some("day"), None, Some("fallback"));
        assert_eq!(
            picked(&no_dark, AppTheme::Dark),
            Some(Some("fallback".into()))
        );
        let no_light = arms(None, Some("night"), Some("fallback"));
        assert_eq!(
            picked(&no_light, AppTheme::Light),
            Some(Some("fallback".into()))
        );

        // With no Default there is nothing to fall back TO: the result is
        // "no arm", which the caller writes as a null (masking lower tiers),
        // rather than the other theme's value.
        let bare = arms(Some("day"), None, None);
        assert!(picked(&bare, AppTheme::Dark).is_none());
    }

    #[test]
    fn an_arm_written_as_null_is_supplied_and_stops_the_fallback() {
        let a = arms(Some("null"), None, Some("fallback"));
        assert_eq!(
            picked(&a, AppTheme::Light),
            Some(None),
            "Light={{x:Null}} must yield null, NOT fall through to Default"
        );
        assert_eq!(picked(&a, AppTheme::Dark), Some(Some("fallback".into())));
    }

    #[test]
    fn the_generation_moves_only_when_the_effective_theme_does() {
        let mut theme = PfAppTheme::default();
        assert_eq!(theme.requested(), AppTheme::Unspecified);

        theme.set_user(AppTheme::Dark);
        assert_eq!((theme.requested(), theme.generation()), (AppTheme::Dark, 1));

        theme.set_user(AppTheme::Dark);
        assert_eq!(
            theme.generation(),
            1,
            "setting the same theme must not fire"
        );

        // The OS says Light while the user insists on Dark: nothing changes.
        theme.set_platform(AppTheme::Light);
        assert_eq!((theme.requested(), theme.generation()), (AppTheme::Dark, 1));

        // Handing control back to the OS reveals the platform theme.
        theme.set_user(AppTheme::Unspecified);
        assert_eq!(
            (theme.requested(), theme.generation()),
            (AppTheme::Light, 2)
        );
    }
}
