//! World-space (diegetic) XAML UI — a XAML surface placed IN the 3D scene
//! rather than over it (replicated
//! from scratch: original XAML and scene, no assets copied).
//!
//! Three XAML panels live INSIDE a 3D world, each attached to a moving
//! object, each demonstrating a different world-space idiom:
//!
//! - **Power core** — a status card that always faces the camera (full
//!   billboard). Its ProgressBar and status line are live `{Binding}`s;
//!   a system oscillates the reactor output and the panel just re-renders.
//! - **Supply crate** — a vendor sign FIXED to the crate (true diegetic UI:
//!   it turns with the object, exactly like a painted sign).
//! - **Field beacon** — an interactive terminal that yaw-billboards
//!   (upright, rotating only around Y). Click the panel in the 3D world
//!   (mesh picking) to ping it: the bound counter updates through the
//!   same view-model as any 2D bevy_pf screen.
//!
//! The recipe per panel: spawn the XAML with `spawn_xaml_bound`, point its
//! root at a `Camera2d` rendering to a transparent offscreen `Image`
//! (`UiTargetCamera` + `RenderTarget::Image`), then map that image onto an
//! unlit quad in the world.
//!
//! Run with: `cargo run -p bevy_pf --example world_space_ui`

use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::ui::UiTargetCamera;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct Vm {
    core_power: f64,
    core_status: String,
    pings: u32,
    ping_note: String,
}

#[derive(Resource)]
struct VmHandle(Bindable);

/// Always face the camera (screen-aligned billboard).
#[derive(Component)]
struct FullBillboard;

/// Stay upright; rotate around Y toward the camera.
#[derive(Component)]
struct YawBillboard;

/// Follow a world anchor at a fixed offset (for billboarded panels, which
/// cannot be children of spinning objects — their rotation is their own).
#[derive(Component)]
struct FollowAnchor {
    anchor: Entity,
    offset: Vec3,
}

#[derive(Component)]
struct Spin(f32);

#[derive(Component)]
struct Bob {
    base_y: f32,
    phase: f32,
}

#[derive(Component)]
struct OrbitCamera;

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "bevy_pf — world-space XAML UI".to_string(),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        window.canvas = Some("#bevy-canvas".to_string());
        window.fit_canvas_to_parent = true;
    }
    window
}

fn main() {
    let mut app = App::new();
    app.add_plugins({
            let plugins = DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window()),
                ..Default::default()
            });
            #[cfg(target_arch = "wasm32")]
            let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
            plugins
        })
        .add_plugins((PfUiPlugin, MeshPickingPlugin))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                orbit_camera,
                spin,
                bob,
                follow_anchors,
                billboards.after(follow_anchors).after(orbit_camera),
                reactor_sim,
            ),
        );
    // WSUI_SHOT=<path.png> captures a frame ~2s in (native verification).
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(path) = std::env::var("WSUI_SHOT") {
        app.add_systems(Update, move |mut commands: Commands,
                                      time: Res<Time>,
                                      mut done: Local<bool>| {
            if !*done && time.elapsed_secs() > 2.0 {
                *done = true;
                commands
                    .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                    .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
            }
        });
    }
    app.run();
}

/// A transparent offscreen image + a 2D camera rendering into it, ready to
/// host one XAML panel.
fn panel_target(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    width: u32,
    height: u32,
) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut image = Image {
        texture_descriptor: TextureDescriptor {
            label: Some("world-space panel"),
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..Default::default()
    };
    image.resize(size);
    let handle = images.add(image);
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..Default::default()
            },
            RenderTarget::Image(handle.clone().into()),
        ))
        .id();
    (handle, camera)
}

/// An unlit, alpha-blended quad showing a panel image, sized in world units.
fn panel_material(image: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(image),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..Default::default()
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let vm = Bindable::new(Vm {
        core_power: 76.0,
        core_status: "STABLE".into(),
        pings: 0,
        ping_note: "click the panel".into(),
    });
    commands.insert_resource(VmHandle(vm.clone()));

    // --- the 3D world -------------------------------------------------
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.2, 9.0).looking_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y),
        OrbitCamera,
        // Ambient light is per-camera in bevy 0.19.
        AmbientLight {
            color: Color::srgb(0.6, 0.7, 0.9),
            brightness: 220.0,
            ..Default::default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..Default::default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Ground.
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(9.0, 0.15))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.13, 0.15, 0.19),
            perceptual_roughness: 0.9,
            ..Default::default()
        })),
        Transform::from_xyz(0.0, -0.075, 0.0),
    ));

    // Anchors: power core (spinning octahedron-ish), supply crate (bobbing,
    // slowly turning cube), field beacon (tall pylon).
    let core = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(0.55).mesh().ico(1).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.9, 0.9),
                emissive: LinearRgba::rgb(0.0, 1.4, 1.6),
                ..Default::default()
            })),
            Transform::from_xyz(-3.4, 1.2, 0.0),
            Spin(1.1),
        ))
        .id();
    let crate_box = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.3, 1.3, 1.3))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.55, 0.4, 0.22),
                perceptual_roughness: 0.8,
                ..Default::default()
            })),
            Transform::from_xyz(3.4, 0.75, 0.6),
            Spin(0.35),
            Bob {
                base_y: 0.75,
                phase: 0.0,
            },
        ))
        .id();
    let beacon = commands
        .spawn((
            Mesh3d(meshes.add(Cylinder::new(0.16, 2.6))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.85, 0.5, 0.15),
                emissive: LinearRgba::rgb(1.4, 0.6, 0.05),
                ..Default::default()
            })),
            Transform::from_xyz(0.2, 1.3, -3.2),
        ))
        .id();

    // --- panel 1: power core status card (full billboard, live bindings) ---
    let (core_img, core_cam) = panel_target(&mut commands, &mut images, 460, 260);
    let core_root = commands.spawn_xaml_bound(
        xaml!(
            r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Background="#E6101826" BorderBrush="#FF35E0E8" BorderThickness="2"
                        CornerRadius="14" Padding="18,12">
                  <StackPanel>
                    <TextBlock Text="POWER CORE" FontSize="26" FontWeight="Bold" Foreground="#FF35E0E8"/>
                    <TextBlock Text="reactor output" FontSize="13" Foreground="#FF9FB6C9" Margin="0,2,0,8"/>
                    <ProgressBar x:Name="CoreBar" Height="18" Minimum="0" Maximum="100"
                                 Value="{Binding core_power}"/>
                    <StackPanel Orientation="Horizontal" Margin="0,8,0,0">
                      <TextBlock Text="status: " FontSize="15" Foreground="#FF9FB6C9"/>
                      <TextBlock Text="{Binding core_status}" FontSize="15" FontWeight="Bold" Foreground="#FFDCF5F7"/>
                    </StackPanel>
                  </StackPanel>
                </Border>"##
        ),
        vm.clone(),
    );
    commands.entity(core_root).insert(UiTargetCamera(core_cam));
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(2.3, 1.3))),
        MeshMaterial3d(materials.add(panel_material(core_img))),
        Transform::from_xyz(-3.4, 2.6, 0.0),
        FullBillboard,
        FollowAnchor {
            anchor: core,
            offset: Vec3::new(0.0, 1.4, 0.0),
        },
    ));

    // --- panel 2: vendor sign, FIXED to the crate (turns with it) ---------
    let (sign_img, sign_cam) = panel_target(&mut commands, &mut images, 420, 300);
    let sign_root = commands.spawn_xaml(xaml!(
        r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Background="#F2241A10" BorderBrush="#FFC98A3B" BorderThickness="3"
                    CornerRadius="8" Padding="16,10">
              <StackPanel>
                <TextBlock Text="SUPPLY DEPOT" FontSize="24" FontWeight="Bold" Foreground="#FFEFC06A" HorizontalAlignment="Center"/>
                <Separator Margin="0,6,0,8"/>
                <DockPanel>
                  <TextBlock DockPanel.Dock="Right" Text="12g" Foreground="#FFEFC06A" FontSize="15"/>
                  <TextBlock Text="ration pack" Foreground="#FFE8D9BE" FontSize="15"/>
                </DockPanel>
                <DockPanel Margin="0,4,0,0">
                  <TextBlock DockPanel.Dock="Right" Text="45g" Foreground="#FFEFC06A" FontSize="15"/>
                  <TextBlock Text="plasma cell" Foreground="#FFE8D9BE" FontSize="15"/>
                </DockPanel>
                <DockPanel Margin="0,4,0,0">
                  <TextBlock DockPanel.Dock="Right" Text="140g" Foreground="#FFEFC06A" FontSize="15"/>
                  <TextBlock Text="hull patch kit" Foreground="#FFE8D9BE" FontSize="15"/>
                </DockPanel>
                <TextBlock Text="painted on the crate - it turns with it" FontSize="11"
                           Foreground="#FF9A8264" Margin="0,8,0,0" HorizontalAlignment="Center"/>
              </StackPanel>
            </Border>"##
    ));
    commands.entity(sign_root).insert(UiTargetCamera(sign_cam));
    let sign_quad = commands
        .spawn((
            Mesh3d(meshes.add(Rectangle::new(1.9, 1.35))),
            MeshMaterial3d(materials.add(panel_material(sign_img))),
            // A child of the crate: local pose just off its front face.
            Transform::from_xyz(0.0, 0.35, 0.72),
        ))
        .id();
    commands.entity(crate_box).add_children(&[sign_quad]);

    // --- panel 3: interactive field terminal (yaw billboard + picking) ----
    let (term_img, term_cam) = panel_target(&mut commands, &mut images, 460, 280);
    let term_root = commands.spawn_xaml_bound(
        xaml!(
            r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Background="#E61A2410" BorderBrush="#FF9BE64A" BorderThickness="2"
                        CornerRadius="10" Padding="18,12">
                  <StackPanel>
                    <TextBlock Text="FIELD TERMINAL" FontSize="24" FontWeight="Bold" Foreground="#FF9BE64A"/>
                    <StackPanel Orientation="Horizontal" Margin="0,10,0,0">
                      <TextBlock Text="pings received: " FontSize="16" Foreground="#FFC5D8A8"/>
                      <TextBlock Text="{Binding pings}" FontSize="16" FontWeight="Bold" Foreground="#FFEFFFE0"/>
                    </StackPanel>
                    <TextBlock Text="{Binding ping_note}" FontSize="13" Foreground="#FF8DA36B" Margin="0,6,0,0"/>
                  </StackPanel>
                </Border>"##
        ),
        vm.clone(),
    );
    commands.entity(term_root).insert(UiTargetCamera(term_cam));
    let vm_for_click = vm.clone();
    commands
        .spawn((
            Mesh3d(meshes.add(Rectangle::new(2.2, 1.35))),
            MeshMaterial3d(materials.add(panel_material(term_img))),
            Transform::from_xyz(0.2, 3.1, -3.2),
            YawBillboard,
            FollowAnchor {
                anchor: beacon,
                offset: Vec3::new(0.0, 1.8, 0.0),
            },
        ))
        .observe(move |_click: On<Pointer<Click>>| {
            vm_for_click.update(|m: &mut Vm| {
                m.pings += 1;
                m.ping_note = format!("last ping acknowledged (#{})", m.pings);
            });
        })
        .observe(|over: On<Pointer<Over>>, mut tfs: Query<&mut Transform>| {
            if let Ok(mut tf) = tfs.get_mut(over.entity) {
                tf.scale = Vec3::splat(1.06);
            }
        })
        .observe(|out: On<Pointer<Out>>, mut tfs: Query<&mut Transform>| {
            if let Ok(mut tf) = tfs.get_mut(out.entity) {
                tf.scale = Vec3::ONE;
            }
        });
}

fn orbit_camera(time: Res<Time>, mut cams: Query<&mut Transform, With<OrbitCamera>>) {
    let t = time.elapsed_secs() * 0.22;
    for mut tf in &mut cams {
        tf.translation = Vec3::new(t.cos() * 9.0, 3.4, t.sin() * 9.0);
        tf.look_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y);
    }
}

fn spin(time: Res<Time>, mut spinners: Query<(&Spin, &mut Transform)>) {
    for (spin, mut tf) in &mut spinners {
        tf.rotate_y(spin.0 * time.delta_secs());
    }
}

fn bob(time: Res<Time>, mut bobbers: Query<(&Bob, &mut Transform)>) {
    for (bob, mut tf) in &mut bobbers {
        tf.translation.y = bob.base_y + ((time.elapsed_secs() + bob.phase) * 1.3).sin() * 0.12;
    }
}

fn follow_anchors(
    anchors: Query<&GlobalTransform>,
    mut followers: Query<(&FollowAnchor, &mut Transform)>,
) {
    for (follow, mut tf) in &mut followers {
        if let Ok(anchor) = anchors.get(follow.anchor) {
            tf.translation = anchor.translation() + follow.offset;
        }
    }
}

type CameraOnly = (With<OrbitCamera>, Without<FullBillboard>, Without<YawBillboard>);

fn billboards(
    cams: Query<&Transform, CameraOnly>,
    mut full: Query<&mut Transform, (With<FullBillboard>, Without<YawBillboard>)>,
    mut yaw: Query<&mut Transform, (With<YawBillboard>, Without<FullBillboard>)>,
) {
    let Ok(cam) = cams.single() else {
        return;
    };
    for mut tf in &mut full {
        // Camera rotation: quad +Z ends up pointing back at the camera.
        tf.rotation = cam.rotation;
    }
    for mut tf in &mut yaw {
        let dir = cam.translation - tf.translation;
        tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
    }
}

/// The "game" driving the power-core panel: output drifts on a sine wave,
/// the status line follows it. Mutating the observable is the whole job —
/// the world-space panel re-renders exactly like a screen-space one.
fn reactor_sim(time: Res<Time>, vm: Res<VmHandle>, mut last: Local<f32>) {
    // ~8 updates a second is plenty for a status readout.
    if time.elapsed_secs() - *last < 0.125 {
        return;
    }
    *last = time.elapsed_secs();
    let t = time.elapsed_secs();
    let power = 62.0 + (t * 0.6).sin() * 30.0 + (t * 2.3).sin() * 6.0;
    vm.0.update(|m: &mut Vm| {
        m.core_power = f64::from(power.clamp(0.0, 100.0));
        m.core_status = match power {
            p if p > 88.0 => "SURGING".into(),
            p if p > 45.0 => "STABLE".into(),
            _ => "LOW OUTPUT".into(),
        };
    });
}
