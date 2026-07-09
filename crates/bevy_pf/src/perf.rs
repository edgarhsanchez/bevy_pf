//! Performance tuning for GUI-first applications.

use bevy::app::{App, First, Last, PostUpdate, PreUpdate, RunFixedMainLoop, SpawnScene, Update};
use bevy::ecs::schedule::{Schedule, SingleThreadedExecutor};

/// Switch the per-frame schedules (main app + render app) to the
/// single-threaded executor.
///
/// GUI scenes produce sub-millisecond frames, where Bevy's default
/// multithreaded executor spends more time dispatching (~2us for each of the
/// ~400 mostly-empty systems DefaultPlugins registers) than the systems spend
/// working. On an M4 Pro this takes an empty bevy_pf scene from ~1,000 FPS to
/// ~3,800 FPS offscreen.
///
/// Call it after `DefaultPlugins`:
///
/// ```ignore
/// let mut app = App::new();
/// app.add_plugins(DefaultPlugins).add_plugins(PfUiPlugin);
/// bevy_pf::perf::tune_schedules_for_gui(&mut app);
/// ```
///
/// Skip this if your app also runs heavy per-frame simulation systems that
/// benefit from parallelism — it trades parallel dispatch for lower fixed
/// overhead, which is the right trade only when frames are dominated by
/// scheduling, not work.
pub fn tune_schedules_for_gui(app: &mut App) {
    fn st(s: &mut Schedule) {
        s.set_executor(SingleThreadedExecutor::new());
    }
    app.edit_schedule(First, st);
    app.edit_schedule(PreUpdate, st);
    app.edit_schedule(Update, st);
    app.edit_schedule(PostUpdate, st);
    app.edit_schedule(Last, st);
    app.edit_schedule(SpawnScene, st);
    app.edit_schedule(RunFixedMainLoop, st);
}

/// [`tune_schedules_for_gui`] plus the render app's `Render`/`Extract`
/// schedules.
///
/// **Headless/offscreen apps only.** With a real window on macOS, swapping
/// the render schedule's executor changes which thread configures the
/// surface, and `raw-window-metal` panics with "can only access NSView on
/// the main thread". Offscreen (`RenderTarget::Image`, no window) there is
/// no surface, and this buys another large cut of dispatch overhead.
pub fn tune_schedules_for_gui_headless(app: &mut App) {
    fn st(s: &mut Schedule) {
        s.set_executor(SingleThreadedExecutor::new());
    }
    tune_schedules_for_gui(app);
    if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
        render_app.edit_schedule(bevy::render::Render, st);
        render_app.edit_schedule(bevy::render::ExtractSchedule, st);
    }
}
