//! ExpenseIt — the official WPF "first desktop application" walkthrough,
//! ported from microsoft/WPF-Samples (MIT) to bevy_pf.
//!
//! Source: `Getting Started/WalkthroughFirstWPFApp` — two `Page`s in a
//! navigation shell, a styled people list, and an expense-report `DataGrid`.
//!
//! Documented deviations from the original XAML (kept minimal):
//! - `NavigationWindow` -> a `Window` hosting `<Frame>`; bevy_pf's journal
//!   provides Navigate/GoBack.
//! - `XmlDataProvider` + `XPath=` bindings -> a reflected Rust view-model
//!   with `Path=` bindings (the same four people and expenses).
//! - `Click="Button_Click"` code-behind -> a Bevy observer that stores the
//!   selection and navigates, exactly what the C# did.
//! - High-contrast `SystemParameters`/`x:Static` accessibility triggers and
//!   the `ImageBrush` watermark are dropped (system-theme machinery).
//! - `HeaderTextStyle` gains an explicit `TargetType="Label"` instead of
//!   `Label.`-qualified setters.
//!
//! Run with: `cargo run -p bevy_pf --example wpf_expense_it`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::components::{PfFrame, PfListBox};
use bevy_pf::prelude::*;

#[derive(Reflect, Default, Clone)]
struct Expense {
    expense_type: String,
    expense_amount: String,
}

#[derive(Reflect, Default, Clone)]
struct Person {
    name: String,
    department: String,
    expenses: Vec<Expense>,
}

#[derive(Reflect, Default)]
struct Vm {
    people: Vec<Person>,
    selected_name: String,
    selected_department: String,
    selected_expenses: Vec<Expense>,
}

fn expense(t: &str, amount: &str) -> Expense {
    Expense {
        expense_type: t.into(),
        expense_amount: amount.into(),
    }
}

/// The sample's `XmlDataProvider` dataset, verbatim.
fn dataset() -> Vec<Person> {
    vec![
        Person {
            name: "Mike".into(),
            department: "Legal".into(),
            expenses: vec![expense("Lunch", "50"), expense("Transportation", "50")],
        },
        Person {
            name: "Lisa".into(),
            department: "Marketing".into(),
            expenses: vec![expense("Document printing", "50"), expense("Gift", "125")],
        },
        Person {
            name: "John".into(),
            department: "Engineering".into(),
            expenses: vec![
                expense("Magazine subscription", "50"),
                expense("New machine", "600"),
                expense("Software", "500"),
            ],
        },
        Person {
            name: "Mary".into(),
            department: "Finance".into(),
            expenses: vec![expense("Dinner", "100")],
        },
    ]
}

#[derive(Resource)]
struct VmHandle(Bindable);

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "ExpenseIt".to_string(),
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
            #[cfg(target_arch = "wasm32")]
            let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
            plugins
        })
        .add_plugins(PfUiPlugin)
        .register_page(
            "ExpenseItHome.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          Title="ExpenseIt - Home">
                      <Grid Margin="10,0,10,10" Background="#FFF6F8FC"
                            ColumnDefinitions="230, *" RowDefinitions="*, Auto, *, Auto">
                        <Label Grid.Column="1" Style="{StaticResource HeaderTextStyle}" Content="View Expense Report"/>
                        <Border Grid.Column="1" Grid.Row="1" Style="{StaticResource ListHeaderStyle}">
                          <Label FontWeight="ExtraBold" Style="{StaticResource ListHeaderTextStyle}" Content="Names"/>
                        </Border>
                        <ListBox x:Name="peopleListBox" Grid.Column="1" Grid.Row="2"
                                 ItemsSource="{Binding people}">
                          <ListBox.ItemTemplate>
                            <DataTemplate>
                              <Label Content="{Binding name}"/>
                            </DataTemplate>
                          </ListBox.ItemTemplate>
                        </ListBox>
                        <Button x:Name="ViewButton" Grid.Column="1" Grid.Row="3"
                                Style="{StaticResource ButtonStyle}" Content="View"/>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "ExpenseReportPage.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          Title="ExpenseIt - View Expense Report">
                      <Grid Background="#FFF6F8FC" ColumnDefinitions="230, *" RowDefinitions="Auto, *">
                        <Label Grid.Column="1" Style="{StaticResource HeaderTextStyle}" Content="Expense Report For:"/>
                        <Grid Margin="10" Grid.Column="1" Grid.Row="1"
                              ColumnDefinitions="*, *" RowDefinitions="Auto, Auto, *">
                          <StackPanel Grid.ColumnSpan="2" Orientation="Horizontal">
                            <Label Style="{StaticResource LabelStyle}" Content="Name:"/>
                            <Label Style="{StaticResource LabelStyle}" Content="{Binding selected_name}"/>
                          </StackPanel>
                          <StackPanel Grid.ColumnSpan="2" Grid.Row="1" Orientation="Horizontal">
                            <Label Style="{StaticResource LabelStyle}" Content="Department:"/>
                            <Label Style="{StaticResource LabelStyle}" Content="{Binding selected_department}"/>
                          </StackPanel>
                          <Grid Grid.ColumnSpan="2" Grid.Row="2" VerticalAlignment="Top" HorizontalAlignment="Left">
                            <DataGrid Width="360" BorderThickness="2" BorderBrush="Black"
                                      ItemsSource="{Binding selected_expenses}"
                                      AutoGenerateColumns="False" HeadersVisibility="Column"
                                      CanUserResizeColumns="False" CanUserResizeRows="False">
                              <DataGrid.Columns>
                                <DataGridTextColumn Header="ExpenseType" Binding="{Binding expense_type}" Width="2*"/>
                                <DataGridTextColumn Header="Amount" Binding="{Binding expense_amount}" Width="*"/>
                              </DataGrid.Columns>
                            </DataGrid>
                          </Grid>
                          <Hyperlink Grid.Row="2" Grid.Column="1" VerticalAlignment="Bottom"
                                     NavigateUri="ExpenseItHome.xaml" Content="Back to names"/>
                        </Grid>
                      </Grid>
                    </Page>"##
            ),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, wire_pages)
        .run();
}

/// The sample's `Styles.xaml`, minus the high-contrast system-theme triggers.
const STYLES: &str = r##"<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                                             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
      <Style x:Key="HeaderTextStyle" TargetType="Label">
        <Setter Property="VerticalAlignment" Value="Center"/>
        <Setter Property="FontFamily" Value="Trebuchet MS"/>
        <Setter Property="FontWeight" Value="Bold"/>
        <Setter Property="FontSize" Value="18"/>
        <Setter Property="Foreground" Value="#183862"/>
      </Style>
      <Style x:Key="LabelStyle" TargetType="Label">
        <Setter Property="VerticalAlignment" Value="Top"/>
        <Setter Property="HorizontalAlignment" Value="Left"/>
        <Setter Property="FontWeight" Value="Bold"/>
        <Setter Property="Margin" Value="0,0,0,5"/>
      </Style>
      <Style x:Key="ListHeaderStyle" TargetType="Border">
        <Setter Property="Padding" Value="5"/>
        <Setter Property="Background" Value="#3274CD"/>
      </Style>
      <Style x:Key="ListHeaderTextStyle" TargetType="Label">
        <Setter Property="Foreground" Value="White"/>
        <Setter Property="VerticalAlignment" Value="Center"/>
        <Setter Property="HorizontalAlignment" Value="Left"/>
      </Style>
      <Style x:Key="ButtonStyle" TargetType="Button">
        <Setter Property="Width" Value="125"/>
        <Setter Property="Margin" Value="0,10,0,0"/>
        <Setter Property="HorizontalAlignment" Value="Right"/>
      </Style>
    </ResourceDictionary>"##;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    // Application.Resources <- Styles.xaml (App.xaml's merged dictionary).
    commands.queue(|world: &mut World| {
        let doc = bevy_pf_xaml::parse(STYLES).expect("styles parse");
        let warnings = bevy_pf::instantiate::set_application_resources(
            world,
            &doc,
            &bevy_pf::XamlEnv::default(),
        );
        for w in warnings {
            warn!("ExpenseIt styles: {w}");
        }
    });

    let vm = Bindable::new(Vm {
        people: dataset(),
        ..Default::default()
    });
    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="ExpenseIt">
                  <Frame x:Name="MainFrame" Source="ExpenseItHome.xaml"/>
                </Window>"##
        ),
        vm.clone(),
    );
    commands.insert_resource(VmHandle(vm));
}

/// Recreates the sample's code-behind: `Button_Click` reads the ListBox
/// selection and navigates to the report page with that person as context.
fn wire_pages(mut navigated: MessageReader<PfNavigated>, ui: PfQuery, mut commands: Commands) {
    for nav in navigated.read() {
        if nav.source != "ExpenseItHome.xaml" {
            continue;
        }
        let Some(button) = ui.by_name("ViewButton") else {
            continue;
        };
        commands
            .entity(button)
            .observe(|_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    // Selected index = the selected row's position in the list.
                    let Some((list, selected)) = world
                        .query::<(Entity, &bevy_pf::components::PfName, &PfListBox)>()
                        .iter(world)
                        .find(|(_, n, _)| n.0 == "peopleListBox")
                        .map(|(e, _, l)| (e, l.selected))
                    else {
                        return;
                    };
                    let index = selected.and_then(|sel| {
                        world
                            .get::<Children>(list)
                            .and_then(|c| c.iter().position(|e| e == sel))
                    });
                    let Some(index) = index else { return }; // nothing selected
                    if let Some(vm) = world.get_resource::<VmHandle>() {
                        let vm = vm.0.clone();
                        vm.update(move |m: &mut Vm| {
                            if let Some(p) = m.people.get(index).cloned() {
                                m.selected_name = p.name;
                                m.selected_department = p.department;
                                m.selected_expenses = p.expenses;
                            }
                        });
                    }
                    let frame = world
                        .query::<(Entity, &bevy_pf::components::PfName, &PfFrame)>()
                        .iter(world)
                        .find(|(_, n, _)| n.0 == "MainFrame")
                        .map(|(e, _, _)| e);
                    if let Some(frame) = frame {
                        bevy_pf::navigation::navigate(world, frame, "ExpenseReportPage.xaml");
                    }
                });
            });
    }
}
