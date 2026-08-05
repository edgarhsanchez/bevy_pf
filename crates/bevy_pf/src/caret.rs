//! Caret blinking.
//!
//! Bevy's `TextCursorStyle` draws a solid, permanently-lit caret. Every
//! platform text control blinks its caret (WPF follows the OS cadence,
//! roughly half a second per phase), so a steady caret reads as broken and
//! the conformance gap is bevy_pf's to close, not each app's.
//!
//! The framework owns the MECHANISM only: a resource with the cadence and a
//! system that modulates the caret colour's alpha. The POLICY is the app's —
//! the default mimics the classic hard blink; a design system that wants a
//! smoother treatment (e.g. Obsidian MK-II's `pulse-block`, opacity 0.3 -> 1
//! alternating each second) overrides the resource:
//!
//! ```ignore
//! app.insert_resource(PfCaretBlink { period_secs: 2.0, min_alpha: 0.3, smooth: true });
//! ```
//!
//! Only alpha is touched. The caret's base COLOUR keeps flowing from the
//! control's `Foreground` (stored in [`PfCaretBase`] at instantiation), so
//! themes and per-control foreground changes keep working unchanged.
//!
//! The caret's WIDTH is not ours to change: the cursor rect comes out of
//! bevy_text's layout (`TextLayoutInfo.cursor`) and is consumed directly by
//! bevy_ui_render's extraction, so a block caret (the other half of the
//! MK-II spec) needs upstream support, not a workaround here.

use bevy::prelude::*;
use bevy::text::TextCursorStyle;

/// Cadence and shape of the caret blink.
#[derive(Resource, Clone, Copy, Debug)]
pub struct PfCaretBlink {
    /// Seconds for one full cycle (visible + hidden, or up + down).
    pub period_secs: f32,
    /// Alpha at the bottom of the cycle. 0.0 disappears entirely (classic
    /// blink); higher values keep the caret readable while it breathes.
    pub min_alpha: f32,
    /// `false`: square wave — visible for half the period, `min_alpha` for
    /// the other half (the classic caret). `true`: triangle wave — a smooth
    /// alternate ramp between `min_alpha` and full, CSS
    /// `animation: 1s infinite alternate` semantics with `period_secs` being
    /// BOTH directions.
    pub smooth: bool,
}

impl Default for PfCaretBlink {
    fn default() -> Self {
        // The classic hard blink at the Windows default cadence
        // (~530 ms per phase).
        Self {
            period_secs: 1.06,
            min_alpha: 0.0,
            smooth: false,
        }
    }
}

/// The caret's authored colour, captured when the control is instantiated.
/// The blink system writes `base.with_alpha(base_alpha * factor)` every
/// frame; without the stored base, the first frame's write would destroy the
/// authored alpha and the pulse would decay to nothing.
#[derive(Component, Clone, Copy, Debug)]
pub struct PfCaretBase(pub Color);

pub(crate) fn blink_carets(
    time: Res<Time>,
    blink: Res<PfCaretBlink>,
    mut carets: Query<(&PfCaretBase, &mut TextCursorStyle)>,
) {
    if carets.is_empty() {
        return;
    }
    let period = blink.period_secs.max(1.0 / 60.0);
    let phase = (time.elapsed_secs() / period).fract();
    let factor = if blink.smooth {
        // Triangle: 0 -> 1 over the first half, back over the second.
        let tri = 1.0 - (phase * 2.0 - 1.0).abs();
        blink.min_alpha + (1.0 - blink.min_alpha) * tri
    } else if phase < 0.5 {
        1.0
    } else {
        blink.min_alpha
    };
    for (base, mut style) in &mut carets {
        let base_alpha = base.0.alpha();
        let target = base.0.with_alpha(base_alpha * factor);
        // Change detection: don't dirty every style every frame at rest.
        if style.color != target {
            style.color = target;
        }
    }
}
