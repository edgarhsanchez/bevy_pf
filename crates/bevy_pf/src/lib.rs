//! # bevy_pf — a XAML / WPF-like UI framework for Bevy
//!
//! Write UI in XAML — inline via the [`xaml!`] macro, or in `.xaml` files —
//! and instantiate it as `bevy_ui` entities, with WPF-style layout panels,
//! resources and styles, and the common WPF control set.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_pf::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(PfUiPlugin)
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2d);
//!     commands.spawn_xaml(xaml!(
//!         r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
//!                        Margin="24" Spacing="8">
//!              <TextBlock Text="Hello from XAML" FontSize="28" FontWeight="Bold"/>
//!              <Button x:Name="Go" Content="Click me" Width="140"
//!                      xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"/>
//!            </StackPanel>"#
//!     ));
//! }
//! ```

pub mod animation;
pub mod asset;
pub mod behaviors;
pub mod binding;
pub mod caret;
pub mod components;
pub mod convert;
pub mod dialog;
pub mod dynamic;
pub mod error;
pub mod events;
pub mod fonts;
pub mod hit_test;
pub mod icons;
pub mod instantiate;
pub mod items;
pub mod navigation;
pub mod overlay;
pub mod perf;
pub mod plugin;
pub mod query;
pub mod provider;
pub mod resources;
pub mod shapes;
#[cfg(feature = "vector_gpu")]
pub mod shapes_gpu;
pub mod themes;
pub mod toast;
pub mod triggers;
pub mod util;

// The `Bindable` DERIVE and the `Bindable` TYPE share a name across
// namespaces, exactly like bevy's `Reflect`: `#[derive(Bindable)]` resolves
// the macro, `Bindable::new` the type.
pub use bevy_pf_macros::{Bindable, include_xaml, xaml};
pub use bevy_pf_xaml as xaml_ast;

pub use asset::{XamlAsset, XamlView};
pub use binding::{Bindable, BoundValue, DataContext, DataContextScope, PfBindings};
pub use components::{ButtonVisual, PfAttachedProps, PfElementKind, PfName, XamlNames};
pub use dynamic::{
    PfApplicationResources, PfDynamicResources, PfResources, merge_application_resources,
    set_application_resources_dict,
};
pub use error::PfError;
pub use events::{PfEventAppExt, PfEventHandlers, PfPointerInfo};
pub use instantiate::{
    InstantiateResult, XamlEnv, instantiate_document, instantiate_document_env,
    set_application_resources,
};
pub use plugin::{PfCommandsExt, PfUiPlugin};
pub use hit_test::{PfHitFilter, PfHitTest, PfHitTestVisible};
pub use items::PfItemsSource;
pub use overlay::{PfPlacement, PfPopup, PfToolTip};
pub use provider::{PfPropertyStore, PropertyTarget, ValueSource};
pub use triggers::PfTriggers;

pub mod prelude {
    pub use crate::asset::{XamlAsset, XamlView};
    pub use bevy_pf_macros::Bindable;
    pub use crate::behaviors::{clear_focus, find_editable_in, focus_control};
    pub use crate::binding::{
        Bindable, DataContext, DataContextScope, PfConverterAppExt, PfMultiValueConverter,
        PfValueConverter,
    };
    pub use crate::components::{PfAutomationId, PfElementKind, PfName, PfUid, XamlNames};
    pub use crate::events::{PfEventAppExt, PfPointerInfo};
    pub use crate::instantiate::PfElementAppExt;
    pub use crate::navigation::{PfNavigationAppExt, PfNavigated};
    pub use crate::hit_test::{PfHitFilter, PfHitTest, PfHitTestVisible};
    pub use crate::query::PfQuery;
    pub use crate::components::PfControlTheme;
    pub use crate::plugin::{PfCommandsExt, PfFocusRingColor, PfUiPlugin};
    pub use crate::{XamlScene, include_xaml, xaml};
}

/// A validated XAML scene, ready to be spawned into a Bevy world.
#[derive(Debug, Clone)]
pub struct XamlScene {
    source: SceneSource,
}

#[derive(Debug, Clone)]
enum SceneSource {
    Static(&'static str),
    Owned(std::sync::Arc<str>),
}

impl XamlScene {
    /// Used by the `xaml!` / `include_xaml!` macros. The source has already
    /// been validated at compile time.
    #[doc(hidden)]
    pub const fn from_static_validated(source: &'static str) -> Self {
        Self {
            source: SceneSource::Static(source),
        }
    }

    /// Create a scene from a runtime string (e.g. a loaded `.xaml` file),
    /// validating it now.
    pub fn parse(source: impl Into<String>) -> Result<Self, PfError> {
        let source: String = source.into();
        bevy_pf_xaml::parse(&source)?;
        Ok(Self {
            source: SceneSource::Owned(source.into()),
        })
    }

    pub fn source(&self) -> &str {
        match &self.source {
            SceneSource::Static(s) => s,
            SceneSource::Owned(s) => s,
        }
    }

    /// Parse the scene into a XAML document. Cannot fail: construction
    /// validates the source.
    pub fn document(&self) -> bevy_pf_xaml::XamlDocument {
        bevy_pf_xaml::parse(self.source()).expect("validated XAML failed to re-parse")
    }
}
