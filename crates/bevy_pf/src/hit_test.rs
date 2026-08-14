//! Asking "what UI is under this point?" from an ordinary system.
//!
//! Pointer *events* answer this for you — an observer on a Button fires
//! because bevy's UI picking backend already did the hit test. But some
//! interactions are about the ABSENCE of a target: light dismiss ("a press
//! on nothing puts the panel away"), drag surfaces that must not steal a
//! press meant for a control, hotspot cursors. Those need to ask the
//! question directly, and doing it by hand is a trap with four separate
//! teeth:
//!
//! 1. **`GlobalTransform` is not a UI component.** Since the UI transform
//!    split, bevy_ui writes [`UiGlobalTransform`] and never touches
//!    `GlobalTransform` — `Node` does not even require it. A
//!    `Query<(&ComputedNode, &GlobalTransform)>` therefore matches *zero*
//!    UI entities, and the failure is silent: the query simply never
//!    yields, so the caller concludes there is no UI anywhere on screen.
//! 2. **[`ComputedNode`] is physical, pointers are logical.**
//!    `Window::cursor_position` and `Touch::position` are logical points;
//!    node geometry is in physical pixels. On any display with a scale
//!    factor above 1 — every phone, every Retina Mac — forgetting the
//!    conversion puts every rect in the wrong place.
//! 3. **`Visibility::Hidden` does not remove a node from layout.** Hidden
//!    panels keep full geometry, so a naive walk finds a closed dialog
//!    sitting over the middle of the screen and reports a hit on it.
//!    (And `Visibility::Visible` on a child *overrides* a hidden parent,
//!    so a subtree cannot simply be pruned.)
//! 4. **Document roots span the window.** bevy_pf mounts every document as
//!    a full-size root, so "is the point inside this document?" is true
//!    everywhere and answers nothing. What a caller almost always means is
//!    "is the point over something that DRAWS" — see [`PfHitFilter`].
//!
//! [`PfHitTest`] is a [`SystemParam`] that gets all four right:
//!
//! ```ignore
//! fn dismiss_on_outside_press(hit: PfHitTest, window: Single<&Window>, ...) {
//!     let Some(press) = window.cursor_position() else { return };
//!     // Nothing painted under the finger: the pointer is on empty scene.
//!     if hit.hit(press, PfHitFilter::Painted).is_none() {
//!         close_the_panel();
//!     }
//! }
//! ```

use bevy::ecs::system::SystemParam;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::text::TextColor;
use bevy::ui::widget::{ImageNode, Text};
use bevy::ui::{BackgroundColor, BorderColor, ComputedNode, UiGlobalTransform};

/// WPF [`UIElement.IsHitTestVisible`]. `False` excludes this element **and
/// every descendant** from hit testing — the WPF walk never descends into a
/// non-hit-test-visible subtree, so an inner `IsHitTestVisible="True"` does
/// not claw its way back out.
///
/// bevy's `Pickable::IGNORE` is per entity and does not propagate, so this
/// component is the declaration and [`propagate_hit_test_visibility`] is
/// what makes it mean what WPF means.
///
/// [`UIElement.IsHitTestVisible`]: https://learn.microsoft.com/dotnet/api/system.windows.uielement.ishittestvisible
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PfHitTestVisible(pub bool);

/// Set on nodes this module opted out of picking, so flipping
/// `IsHitTestVisible` back on restores exactly what it suppressed and never
/// re-enables an opt-out the application made for its own reasons.
#[derive(Component, Debug)]
pub struct PfHitTestSuppressed;

/// Whether a `Background` brush was ever assigned to this element, at any
/// precedence tier.
///
/// WPF distinguishes a **null** `Background` (nothing is rendered, so
/// nothing is hit) from `Transparent` (a real brush that renders nothing
/// visible but IS hit) — the distinction behind the `Background="Transparent"`
/// idiom. Both collapse to `BackgroundColor(Color::NONE)` in bevy, so the
/// fact has to be recorded separately. Maintained by
/// [`crate::provider::apply_value`] / `apply_unset`, which are the single
/// writers for the property.
#[derive(Component, Debug)]
pub struct PfBackgroundSet;

/// Elements whose hit-testing WPF governs purely by their `Background`
/// brush: `Panel` subclasses render nothing else, so with a null Background
/// a click passes straight through to whatever is behind.
///
/// This is the rule that keeps a layout `Grid` wrapped around a screen from
/// swallowing every click aimed beneath it. `Border` is deliberately absent:
/// WPF hits a null-Background Border on its border ring only, a shape bevy's
/// rectangular picking cannot express, so it stays hit-testable.
///
/// `Window` is governed too, but WPF gives it a white Background by default
/// (as does [`crate::instantiate`]), so window roots stay hit-testable.
pub(crate) fn background_governs_hit_testing(kind: &str) -> bool {
    matches!(
        kind,
        "Grid"
            | "StackPanel"
            | "DockPanel"
            | "WrapPanel"
            | "Canvas"
            | "UniformGrid"
            | "Window"
            | "Page"
            | "UserControl"
    )
}

/// Which nodes count as a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PfHitFilter {
    /// Every node that is laid out and visible, painted or not.
    ///
    /// Full-window document wrappers and transparent layout panels match,
    /// so this answers "is the point inside this element's box" — useful
    /// against a *known* element, misleading against a whole scene.
    Visible,
    /// Visible, and able to receive a pointer event: not opted out with
    /// [`Pickable::IGNORE`].
    ///
    /// The WPF `IsHitTestVisible` question — "would a click here be
    /// delivered to this tree?" — and so the analogue of WPF's
    /// `UIElement.InputHitTest`.
    HitTestVisible,
    /// Visible, and actually drawing something: a background or border
    /// with any alpha at all, text, or an image.
    ///
    /// The "is there UI under my finger" question, and the default because
    /// it is the one that separates a control from the invisible wrapper
    /// stretched across the window behind it. Note this is deliberately
    /// independent of pickability: a passive readout is still something
    /// the user can see and point at, even though it takes no clicks.
    #[default]
    Painted,
}

/// Everything a hit test reads off one node: geometry and transform, the
/// visibility it resolves through, and the four ways a node can paint
/// (background, border, text, image).
type HitNode = (
    &'static ComputedNode,
    &'static UiGlobalTransform,
    &'static Visibility,
    &'static BackgroundColor,
    &'static BorderColor,
    Option<&'static Pickable>,
    Option<&'static TextColor>,
    Has<Text>,
    Has<ImageNode>,
);

/// The pair WPF's Panel rule reads back: which element this is, and whether
/// a `Background` brush is currently assigned to it.
type GovernedNode = (
    Option<&'static crate::components::PfElementKind>,
    Has<PfBackgroundSet>,
);

/// Hit-testing over the UI tree, in logical window coordinates.
///
/// Every method takes `point` in the same space as
/// [`Window::cursor_position`](bevy::window::Window::cursor_position) and
/// [`Touch::position`](bevy::input::touch::Touch::position): logical
/// pixels, origin at the window's top-left. The physical conversion is
/// done per node from its own scale factor.
#[derive(SystemParam)]
pub struct PfHitTest<'w, 's> {
    nodes: Query<'w, 's, HitNode>,
    children: Query<'w, 's, &'static Children>,
    // UI roots: a laid-out node with no parent. Documents mounted by
    // `instantiate_document` and the overlay layer both land here.
    roots: Query<'w, 's, Entity, (With<ComputedNode>, Without<ChildOf>)>,
}

impl PfHitTest<'_, '_> {
    /// The topmost matching node under `point`, anywhere in the UI.
    ///
    /// "Topmost" is resolved within each root by paint order (later
    /// siblings and deeper descendants win). Across independent roots the
    /// first match is returned; when several roots overlap and the
    /// distinction matters, ask each one with [`Self::hit_in`] in the
    /// order you care about.
    pub fn hit(&self, point: Vec2, filter: PfHitFilter) -> Option<Entity> {
        self.roots
            .iter()
            .find_map(|root| self.descend(root, point, filter, true))
    }

    /// The topmost matching node under `point` within `root`'s subtree.
    ///
    /// `root` itself is eligible. It need not be a UI node — a bare
    /// logical wrapper is walked through to its children.
    pub fn hit_in(&self, root: Entity, point: Vec2, filter: PfHitFilter) -> Option<Entity> {
        self.descend(root, point, filter, true)
    }

    /// Whether anything in `root`'s subtree matches under `point`.
    pub fn contains(&self, root: Entity, point: Vec2, filter: PfHitFilter) -> bool {
        self.hit_in(root, point, filter).is_some()
    }

    /// Whether anything in the whole UI matches under `point`.
    pub fn any(&self, point: Vec2, filter: PfHitFilter) -> bool {
        self.hit(point, filter).is_some()
    }

    /// Children are visited last-to-first (paint order is first-to-last,
    /// so the last sibling is on top) and before the node itself, which
    /// makes the returned entity the deepest, topmost match.
    ///
    /// `inherited` is the effective visibility of the parent chain. The
    /// subtree is NOT pruned when a node is hidden: bevy resolves
    /// `Visibility::Visible` on a descendant as visible regardless of an
    /// ancestor being hidden, and this mirrors that exactly.
    fn descend(
        &self,
        entity: Entity,
        point: Vec2,
        filter: PfHitFilter,
        inherited: bool,
    ) -> Option<Entity> {
        let visible = match self.nodes.get(entity) {
            Ok((_, _, visibility, _, _, _, _, _, _)) => match visibility {
                Visibility::Visible => true,
                Visibility::Hidden => false,
                Visibility::Inherited => inherited,
            },
            // Not a UI node; it cannot be hit, but it can still parent
            // nodes that can.
            Err(_) => inherited,
        };

        if let Ok(children) = self.children.get(entity) {
            let kids: Vec<Entity> = children.iter().collect();
            for kid in kids.into_iter().rev() {
                if let Some(found) = self.descend(kid, point, filter, visible) {
                    return Some(found);
                }
            }
        }

        if !visible {
            return None;
        }
        let Ok((node, transform, _, background, border, pickable, text_color, has_text, has_image)) =
            self.nodes.get(entity)
        else {
            return None;
        };
        let qualifies = match filter {
            PfHitFilter::Visible => true,
            PfHitFilter::HitTestVisible => {
                // Absent `Pickable` is bevy's "blocks and hovers" default.
                pickable.is_none_or(|p| p.should_block_lower || p.is_hoverable)
            }
            PfHitFilter::Painted => {
                let paints_background = !background.0.is_fully_transparent();
                let thickness = node.border();
                let paints_border = !border.is_fully_transparent()
                    && (thickness.min_inset.max_element() > 0.0
                        || thickness.max_inset.max_element() > 0.0);
                let paints_text =
                    has_text && text_color.is_none_or(|c| !c.0.is_fully_transparent());
                paints_background || paints_border || paints_text || has_image
            }
        };
        if !qualifies {
            return None;
        }
        // ComputedNode/UiGlobalTransform are PHYSICAL; `point` is LOGICAL.
        let inverse_scale = node.inverse_scale_factor();
        if inverse_scale <= 0.0 {
            return None;
        }
        node.contains_point(*transform, point / inverse_scale)
            .then_some(entity)
    }
}

/// Only walk the tree when an `IsHitTestVisible` declaration exists AND
/// something that could change its reach has moved: the flag itself, or new
/// nodes (an `ItemsControl` generating rows into a suppressed subtree).
pub(crate) fn hit_test_visibility_is_stale(
    declared: Query<(), With<PfHitTestVisible>>,
    changed: Query<(), Changed<PfHitTestVisible>>,
    added: Query<(), Added<Node>>,
) -> bool {
    !declared.is_empty() && (!changed.is_empty() || !added.is_empty())
}

/// Give `IsHitTestVisible="False"` its WPF reach: the element and every
/// descendant drop out of hit testing.
///
/// One top-down pass, mirroring how bevy propagates `Visibility` — and like
/// that pass it is the only writer of what it owns, tracked by
/// [`PfHitTestSuppressed`] so an application's own `Pickable` choices
/// survive untouched.
pub(crate) fn propagate_hit_test_visibility(
    roots: Query<Entity, (With<Node>, Without<ChildOf>)>,
    flags: Query<&PfHitTestVisible>,
    children: Query<&Children>,
    suppressed: Query<(), With<PfHitTestSuppressed>>,
    governed: Query<GovernedNode>,
    mut commands: Commands,
) {
    /// What a node's pickability should be once this pass stops suppressing
    /// it — which is NOT unconditionally the default: a background-governed
    /// Panel with a null Background is opted out for its own WPF reason, and
    /// restoring must not undo that.
    fn restored_pickable(entity: Entity, governed: &Query<GovernedNode>) -> Pickable {
        match governed.get(entity) {
            Ok((Some(kind), false)) if background_governs_hit_testing(&kind.0) => Pickable::IGNORE,
            _ => Pickable::default(),
        }
    }

    fn walk(
        entity: Entity,
        inherited: bool,
        flags: &Query<&PfHitTestVisible>,
        children: &Query<&Children>,
        suppressed: &Query<(), With<PfHitTestSuppressed>>,
        governed: &Query<GovernedNode>,
        commands: &mut Commands,
    ) {
        // WPF: the walk simply never enters a false subtree, so a nested
        // `IsHitTestVisible="True"` cannot re-expose itself.
        let effective = inherited && flags.get(entity).map_or(true, |f| f.0);
        let was_suppressed = suppressed.contains(entity);
        if !effective && !was_suppressed {
            commands
                .entity(entity)
                .insert((Pickable::IGNORE, PfHitTestSuppressed));
        } else if effective && was_suppressed {
            commands
                .entity(entity)
                .remove::<PfHitTestSuppressed>()
                .insert(restored_pickable(entity, governed));
        }
        if let Ok(kids) = children.get(entity) {
            for kid in kids.iter().collect::<Vec<_>>() {
                walk(
                    kid, effective, flags, children, suppressed, governed, commands,
                );
            }
        }
    }

    for root in roots.iter() {
        walk(
            root,
            true,
            &flags,
            &children,
            &suppressed,
            &governed,
            &mut commands,
        );
    }
}
