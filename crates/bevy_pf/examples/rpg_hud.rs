//! A complete RPG HUD kit — every element is XAML, every behavior a system.
//!
//! Follows the conventions players expect from WoW/Diablo-style HUDs:
//! portrait + health(green)/mana(blue) bars top-left, minimap + timed buffs
//! top-right, quest tracker on the right edge, an 8-slot action bar with
//! keybinds and cooldown sweeps bottom-center over a purple XP bar, an NPC
//! dialog box bottom-left, and rarity-colored loot toasts
//! (gray → white → green → blue → purple → orange).
//!
//! A tiny combat simulation drives everything: damage/regen, casts, XP and
//! level-ups, cooldowns, buff timers, loot drops. Bar fills are `Border`s
//! whose width a system sets; texts are `{Binding}`s into a reflected VM.
//!
//! Run with: `cargo run -p bevy_pf --example rpg_hud`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::widget::Text;
use bevy_pf::prelude::*;

const HEALTH: &str = "#FF3FBF4A";
const MANA: &str = "#FF3B6FD4";
const XP: &str = "#FF9B59D0";
const GOLD: &str = "#FFC8A24B";
const PANEL: &str = "#D0141008";
const RARITY: [(&str, &str); 6] = [
    ("#FF9D9D9D", "Worn Dagger"),
    ("#FFFFFFFF", "Linen Cloth"),
    ("#FF1EFF00", "Sturdy Boots"),
    ("#FF0070DD", "Sapphire Ring"),
    ("#FFA335EE", "Shadow Cowl"),
    ("#FFFF8000", "Dragonfang Blade"),
];

#[derive(Reflect, Default)]
struct Vm {
    name: String,
    level: u32,
    hp_text: String,
    mana_text: String,
    xp_text: String,
    quest_title: String,
    quest_a: String,
    quest_b: String,
    npc_line: String,
}

#[derive(Resource)]
struct Sim {
    hp: f32,
    hp_max: f32,
    mana: f32,
    mana_max: f32,
    xp: f32,
    xp_max: f32,
    level: u32,
    kills: u32,
    cooldowns: [f32; 8], // seconds remaining
    buffs: [f32; 3],
    loot_timer: f32,
    toast_timer: f32,
    dialog_timer: f32,
    next_loot: usize,
}

#[derive(Resource)]
struct VmHandle(Bindable);

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "bevy_pf RPG HUD".to_string(),
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
    App::new()
        .add_plugins({
            let plugins = DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window()),
                ..Default::default()
            });
            // No demo plays audio; skipping the plugin on the web avoids the
            // browser's AudioContext autoplay warning.
            #[cfg(target_arch = "wasm32")]
            let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
            plugins
        })
        .add_plugins(PfUiPlugin)
        .insert_resource(Sim {
            hp: 86.0,
            hp_max: 100.0,
            mana: 40.0,
            mana_max: 60.0,
            xp: 20.0,
            xp_max: 100.0,
            level: 7,
            kills: 0,
            cooldowns: [0.0; 8],
            buffs: [18.0, 45.0, 9.0],
            loot_timer: 3.0,
            toast_timer: 0.0,
            dialog_timer: 6.0,
            next_loot: 0,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (simulate, paint_bars).chain())
        .run();
}

fn bar(name: &str, color: &str, width: u32) -> String {
    let vm = format!("{}_text", name.to_lowercase());
    format!(
        r##"<Border Width="{width}" Height="18" Background="#B0000000" BorderBrush="{GOLD}" BorderThickness="1" CornerRadius="3" Margin="0,3,0,0">
              <Grid>
                <Border x:Name="{name}Fill" Background="{color}" CornerRadius="2" HorizontalAlignment="Left" Width="100"/>
                <TextBlock Text="{{Binding {vm}}}" Foreground="White" FontSize="11" HorizontalAlignment="Center" VerticalAlignment="Center"/>
              </Grid>
            </Border>"##,
    )
}

fn scene() -> String {
    // 8 action slots with keybinds and a cooldown sweep overlay each.
    let slots: String = (0..8)
        .map(|i| {
            format!(
                r##"<Border Width="46" Height="46" Margin="3,0" Background="#C01A1410" BorderBrush="{GOLD}" BorderThickness="1" CornerRadius="4">
                      <Grid>
                        <TextBlock Text="{glyph}" FontSize="20" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                        <Border x:Name="Cd{i}" Background="#A0000000" VerticalAlignment="Bottom" Height="0"/>
                        <TextBlock Text="{key}" Foreground="#FFD9C07A" FontSize="10" HorizontalAlignment="Right" VerticalAlignment="Top" Margin="0,2,4,0"/>
                      </Grid>
                    </Border>"##,
                glyph = ["A", "S", "D", "F", "G", "H", "J", "K"][i],
                key = i + 1,
            )
        })
        .collect();

    let buffs: String = (0..3)
        .map(|i| {
            format!(
                r##"<Border Width="34" Height="34" Margin="3,0" Background="#C01A2A1A" BorderBrush="#FF5FA75F" BorderThickness="1" CornerRadius="4">
                      <Grid>
                        <TextBlock Text="{glyph}" FontSize="14" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                        <TextBlock x:Name="Buff{i}" Text="" Foreground="White" FontSize="9" HorizontalAlignment="Center" VerticalAlignment="Bottom"/>
                      </Grid>
                    </Border>"##,
                glyph = ["+", "*", "&"][i],
            )
        })
        .collect();

    format!(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Background="#FF1B2430">

              <!-- top-left: portrait + vitals -->
              <StackPanel Orientation="Horizontal" HorizontalAlignment="Left" VerticalAlignment="Top" Margin="16">
                <Border Width="64" Height="64" CornerRadius="32" Background="#FF3A2E1E" BorderBrush="{GOLD}" BorderThickness="2">
                  <Grid>
                    <TextBlock Text="T" FontWeight="Bold" FontSize="28" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                    <Border Background="{GOLD}" CornerRadius="8" Width="22" Height="16" HorizontalAlignment="Right" VerticalAlignment="Bottom">
                      <TextBlock Text="{{Binding level}}" Foreground="#FF14100A" FontSize="10" FontWeight="Bold" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                    </Border>
                  </Grid>
                </Border>
                <StackPanel Margin="10,4,0,0">
                  <TextBlock Text="{{Binding name}}" Foreground="#FFF2E3B8" FontSize="15" FontWeight="Bold"/>
                  {hp_bar}
                  {mana_bar}
                </StackPanel>
              </StackPanel>

              <!-- top-right: minimap + buffs -->
              <StackPanel HorizontalAlignment="Right" VerticalAlignment="Top" Margin="16">
                <Border Width="120" Height="120" CornerRadius="60" Background="#C0202A20" BorderBrush="{GOLD}" BorderThickness="2" HorizontalAlignment="Right">
                  <Grid>
                    <TextBlock Text="N" Foreground="#FFD9C07A" FontSize="12" HorizontalAlignment="Center" VerticalAlignment="Top" Margin="0,4,0,0"/>
                    <TextBlock Text="@" Foreground="#FF7FD4FF" FontSize="14" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                  </Grid>
                </Border>
                <StackPanel Orientation="Horizontal" HorizontalAlignment="Right" Margin="0,8,0,0">{buffs}</StackPanel>
              </StackPanel>

              <!-- right edge: quest tracker -->
              <Border HorizontalAlignment="Right" VerticalAlignment="Center" Margin="0,0,16,0" Background="{PANEL}" BorderBrush="{GOLD}" BorderThickness="1" CornerRadius="6" Padding="10" Width="240">
                <StackPanel>
                  <TextBlock Text="{{Binding quest_title}}" Foreground="#FFF2C94C" FontSize="13" FontWeight="Bold"/>
                  <TextBlock Text="{{Binding quest_a}}" Foreground="#FFE0E0E0" FontSize="12" Margin="0,6,0,0"/>
                  <TextBlock Text="{{Binding quest_b}}" Foreground="#FF9F9F9F" FontSize="12" Margin="0,2,0,0"/>
                </StackPanel>
              </Border>

              <!-- bottom-left: NPC dialog box -->
              <Border x:Name="NpcBox" HorizontalAlignment="Left" VerticalAlignment="Bottom" Margin="16,0,0,90" Background="{PANEL}" BorderBrush="{GOLD}" BorderThickness="1" CornerRadius="6" Padding="12" Width="340">
                <StackPanel>
                  <TextBlock Text="Elder Maren" Foreground="#FFF2C94C" FontSize="13" FontWeight="Bold"/>
                  <TextBlock Text="{{Binding npc_line}}" Foreground="#FFE8E8E8" FontSize="12" TextWrapping="Wrap" Margin="0,4,0,0"/>
                  <TextBlock Text="[Space] Continue" Foreground="#FF8F8F8F" FontSize="10" HorizontalAlignment="Right" Margin="0,6,0,0"/>
                </StackPanel>
              </Border>

              <!-- loot toast -->
              <Border x:Name="Toast" HorizontalAlignment="Center" VerticalAlignment="Top" Margin="0,90,0,0" Background="{PANEL}" BorderBrush="#FF444444" BorderThickness="1" CornerRadius="4" Padding="10,6,10,6">
                <TextBlock x:Name="ToastText" Text="" FontSize="13" FontWeight="Bold"/>
              </Border>

              <!-- bottom-center: action bar + XP strip -->
              <StackPanel HorizontalAlignment="Center" VerticalAlignment="Bottom" Margin="0,0,0,14">
                <StackPanel Orientation="Horizontal" HorizontalAlignment="Center">{slots}</StackPanel>
                <Border Width="404" Height="10" Background="#B0000000" BorderBrush="{GOLD}" BorderThickness="1" CornerRadius="3" Margin="0,6,0,0">
                  <Border x:Name="XpFill" Background="{XP}" CornerRadius="2" HorizontalAlignment="Left" Width="60"/>
                </Border>
                <TextBlock Text="{{Binding xp_text}}" Foreground="#FFB49BD4" FontSize="10" HorizontalAlignment="Center" Margin="0,2,0,0"/>
              </StackPanel>
            </Grid>"##,
        hp_bar = bar("Hp", HEALTH, 220),
        mana_bar = bar("Mana", MANA, 220),
    )
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm {
        name: "Thalia Emberweave".into(),
        level: 7,
        quest_title: "The Ashen Road".into(),
        ..Default::default()
    });
    let scene = XamlScene::parse(scene()).expect("HUD scene is valid XAML");
    commands.spawn_xaml_bound(scene, vm.clone());
    commands.insert_resource(VmHandle(vm));
}

/// The combat simulation: everything the HUD displays comes from here.
fn simulate(time: Res<Time>, mut sim: ResMut<Sim>, vm: Res<VmHandle>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    // Vitals: rhythmic damage, steady regen; casts drain mana.
    sim.hp = (sim.hp + 4.0 * dt - if (t % 5.0) < 0.05 { 18.0 } else { 0.0 })
        .clamp(12.0, sim.hp_max);
    sim.mana = (sim.mana + 3.0 * dt - if (t % 7.0) < 0.05 { 20.0 } else { 0.0 })
        .clamp(0.0, sim.mana_max);

    // XP + level-ups.
    sim.xp += 3.5 * dt;
    if sim.xp >= sim.xp_max {
        sim.xp = 0.0;
        sim.level += 1;
        sim.xp_max *= 1.2;
    }

    // Cooldowns: fire an ability periodically, tick the rest down.
    for i in 0..8 {
        sim.cooldowns[i] = (sim.cooldowns[i] - dt).max(0.0);
    }
    if (t % 2.5) < 0.05 {
        let slot = ((t / 2.5) as usize) % 8;
        if sim.cooldowns[slot] <= 0.0 {
            sim.cooldowns[slot] = 6.0 + slot as f32;
        }
    }

    // Buffs tick down and refresh.
    for i in 0..3 {
        sim.buffs[i] -= dt;
        if sim.buffs[i] <= 0.0 {
            sim.buffs[i] = [24.0, 60.0, 14.0][i];
        }
    }

    // Loot toasts every few seconds, visible for 3s.
    sim.loot_timer -= dt;
    if sim.loot_timer <= 0.0 {
        sim.loot_timer = 5.0;
        sim.toast_timer = 3.0;
        sim.next_loot = (sim.next_loot + 1) % RARITY.len();
        sim.kills += 1;
    } else {
        sim.toast_timer -= dt;
    }
    sim.dialog_timer -= dt;
    if sim.dialog_timer <= 0.0 {
        sim.dialog_timer = 12.0;
    }

    let (hp, hp_max, mana, mana_max) = (sim.hp, sim.hp_max, sim.mana, sim.mana_max);
    let (xp, xp_max, level, kills) = (sim.xp, sim.xp_max, sim.level, sim.kills);
    vm.0.update(move |m: &mut Vm| {
        m.level = level;
        m.hp_text = format!("{:.0} / {:.0}", hp, hp_max);
        m.mana_text = format!("{:.0} / {:.0}", mana, mana_max);
        m.xp_text = format!("Level {level}  -  {xp:.0} / {xp_max:.0} XP");
        m.quest_a = format!("[{}] Slay ash wolves ({}/8)", if kills >= 8 { "x" } else { " " }, kills.min(8));
        m.quest_b = "[ ] Report to Elder Maren".to_string();
        m.npc_line =
            "The road ahead burns, child. Take this ward - and mind the wolves.".to_string();
    });
}

/// Push simulation state into the XAML: fill widths, sweeps, timers, toasts.
fn paint_bars(
    sim: Res<Sim>,
    ui: PfQuery,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut colors: Query<&mut bevy::text::TextColor>,
) {
    let mut set_fill = |name: &str, frac: f32, full: f32| {
        if let Some(e) = ui.by_name(name)
            && let Ok(mut n) = nodes.get_mut(e)
        {
            n.width = Val::Px(full * frac.clamp(0.0, 1.0));
        }
    };
    set_fill("HpFill", sim.hp / sim.hp_max, 218.0);
    set_fill("ManaFill", sim.mana / sim.mana_max, 218.0);
    set_fill("XpFill", sim.xp / sim.xp_max, 402.0);

    // Cooldown sweeps: overlay height shrinks as the cooldown expires.
    for i in 0..8 {
        let frac = (sim.cooldowns[i] / (6.0 + i as f32)).clamp(0.0, 1.0);
        if let Some(e) = ui.by_name(&format!("Cd{i}"))
            && let Ok(mut n) = nodes.get_mut(e)
        {
            n.height = Val::Px(44.0 * frac);
        }
    }

    // Buff timers.
    for i in 0..3 {
        if let Some(e) = ui.by_name(&format!("Buff{i}"))
            && let Some(te) = ui.first_text_in(e)
            && let Ok(mut t) = texts.get_mut(te)
        {
            t.0 = format!("{:.0}s", sim.buffs[i].max(0.0));
        }
    }

    // Loot toast: rarity-colored, only while fresh.
    if let Some(toast) = ui.by_name("Toast")
        && let Ok(mut n) = nodes.get_mut(toast)
    {
        n.display = if sim.toast_timer > 0.0 { Display::Flex } else { Display::None };
    }
    if sim.toast_timer > 0.0
        && let Some(label) = ui.by_name("ToastText")
        && let Some(te) = ui.first_text_in(label)
    {
        let (hex, item) = RARITY[sim.next_loot];
        if let Ok(mut t) = texts.get_mut(te) {
            t.0 = format!("You receive loot: [{item}]");
        }
        if let Ok(mut c) = colors.get_mut(te) {
            let h = hex.trim_start_matches("#FF");
            c.0 = Color::srgb_u8(
                u8::from_str_radix(&h[0..2], 16).unwrap_or(255),
                u8::from_str_radix(&h[2..4], 16).unwrap_or(255),
                u8::from_str_radix(&h[4..6], 16).unwrap_or(255),
            );
        }
    }

    // NPC dialog box: visible for the first 6s of every 12s cycle.
    if let Some(npc) = ui.by_name("NpcBox")
        && let Ok(mut n) = nodes.get_mut(npc)
    {
        n.display = if sim.dialog_timer > 6.0 { Display::Flex } else { Display::None };
    }
}
