//! A gallery of official WPF samples (microsoft/WPF-Samples, MIT) ported to
//! bevy_pf — each sample is a `Page` navigated through a `Frame`, so the
//! gallery itself dogfoods WPF navigation.
//!
//! Ported in this batch (deviations noted per page):
//! - Getting Started: HelloWorld, SimpleLayout, ComplexLayout, DynamicLayout
//!   (Click code-behind -> observer + bound list), MultiPage (inline
//!   `<Hyperlink>` runs inside `TextBlock`, navigating pages).
//! - Data Binding: SimpleBinding (`local:Person` resource -> DataContext),
//!   DirectionalBinding (OneTime/OneWay/TwoWay matrix; `local:NetIncome` ->
//!   DataContext), DataBindingToStringFormat (`{0:c}` currency columns;
//!   the MultiBinding half is out of scope).
//!
//! Run with: `cargo run -p bevy_pf --example wpf_samples_gallery`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

#[derive(Reflect, Default, Clone)]
struct SaleItem {
    description: String,
    price: f64,
}

#[derive(Reflect, Default)]
struct Vm {
    // SimpleBinding's local:Person
    person_name: String,
    // DirectionalBinding's local:NetIncome
    total_income: f64,
    rent: f64,
    food: String,
    misc: String,
    savings: f64,
    // DynamicLayout's appended lines
    lines: Vec<String>,
    // StringFormat's ItemsForSale
    items_for_sale: Vec<SaleItem>,
}

#[derive(Resource)]
struct VmHandle(Bindable);

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "WPF-Samples gallery on bevy_pf".to_string(),
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
            "home.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="WPF-Samples">
                      <StackPanel Margin="24" Background="White">
                        <TextBlock Text="Official WPF samples, running on bevy_pf" FontSize="24" FontWeight="Bold"/>
                        <TextBlock Text="Each page below is ported from microsoft/WPF-Samples with minimal, documented deviations." Margin="0,4,0,12"/>
                        <TextBlock FontWeight="Bold" Text="Getting Started" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="HelloWorld.xaml" Margin="12,2,0,0">HelloWorld</Hyperlink>
                        <Hyperlink NavigateUri="SimpleLayout.xaml" Margin="12,2,0,0">SimpleLayout</Hyperlink>
                        <Hyperlink NavigateUri="ComplexLayout.xaml" Margin="12,2,0,0">ComplexLayout</Hyperlink>
                        <Hyperlink NavigateUri="DynamicLayout.xaml" Margin="12,2,0,0">DynamicLayout</Hyperlink>
                        <Hyperlink NavigateUri="Page1.xaml" Margin="12,2,0,0">MultiPage</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Data Binding" Margin="0,12,0,4"/>
                        <Hyperlink NavigateUri="SimpleBinding.xaml" Margin="12,2,0,0">SimpleBinding</Hyperlink>
                        <Hyperlink NavigateUri="DirectionalBinding.xaml" Margin="12,2,0,0">DirectionalBinding</Hyperlink>
                        <Hyperlink NavigateUri="StringFormat.xaml" Margin="12,2,0,0">DataBindingToStringFormat</Hyperlink>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "HelloWorld.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="HelloWorld">
                      <Grid Background="White">
                        <TextBlock VerticalAlignment="Center" HorizontalAlignment="Center">
                          Hello, World!
                        </TextBlock>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "SimpleLayout.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="SimpleLayout">
                      <StackPanel Background="White" VerticalAlignment="Center" HorizontalAlignment="Center">
                        <Button HorizontalAlignment="Left" Width="100" Margin="10,10,10,10">Button 1</Button>
                        <Button HorizontalAlignment="Left" Width="100" Margin="10,10,10,10">Button 2</Button>
                        <Button HorizontalAlignment="Left" Width="100" Margin="10,10,10,10">Button 3</Button>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "ComplexLayout.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="ComplexLayout">
                      <DockPanel Background="White">
                        <TextBlock Background="LightBlue" DockPanel.Dock="Top">Some Text</TextBlock>
                        <TextBlock DockPanel.Dock="Bottom" Background="LightYellow">Some text at the bottom of the page.</TextBlock>
                        <TextBlock DockPanel.Dock="Left" Background="Lavender">Some More Text</TextBlock>
                        <DockPanel Background="Bisque">
                          <StackPanel DockPanel.Dock="Top">
                            <Button HorizontalAlignment="Left" Height="30px" Width="100px" Margin="10,10,10,10">Button1</Button>
                            <Button HorizontalAlignment="Left" Height="30px" Width="100px" Margin="10,10,10,10">Button2</Button>
                          </StackPanel>
                          <TextBlock Background="LightGreen">Some Text Below the Buttons</TextBlock>
                        </DockPanel>
                      </DockPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "DynamicLayout.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="DynamicLayout">
                      <StackPanel Background="White" VerticalAlignment="Center" HorizontalAlignment="Center">
                        <Button x:Name="Button1" HorizontalAlignment="Left" Width="100"
                                Margin="10,10,10,10" Click="HandleClick">Click Me</Button>
                        <ItemsControl ItemsSource="{Binding lines}"/>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "Page1.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Page1">
                      <StackPanel Background="LightBlue">
                        <TextBlock Margin="10,10,10,10">Start Page</TextBlock>
                        <TextBlock HorizontalAlignment="Left" Margin="10,10,10,10">
                          <Hyperlink NavigateUri="Page2.xaml">Go To Page 2</Hyperlink>
                        </TextBlock>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "Page2.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Page2">
                      <StackPanel Background="LightGreen">
                        <TextBlock Margin="10,10,10,10">Page 2</TextBlock>
                        <TextBlock HorizontalAlignment="Left" Margin="10,10,10,10">
                          <Hyperlink NavigateUri="Page1.xaml">Go To The Start Page</Hyperlink>
                        </TextBlock>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "SimpleBinding.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="SimpleBinding">
                      <Grid Background="White">
                        <Border Margin="5" BorderBrush="Aqua" BorderThickness="1" Padding="8" CornerRadius="3"
                                HorizontalAlignment="Center" VerticalAlignment="Center">
                          <StackPanel Width="220" Margin="16">
                            <Label Content="Enter a Name:"/>
                            <TextBox Width="150" HorizontalAlignment="Left"
                                     Text="{Binding person_name, UpdateSourceTrigger=PropertyChanged}"/>
                            <Label Content="The name you entered:" Margin="0,8,0,0"/>
                            <TextBlock Width="150" Text="{Binding person_name}"/>
                          </StackPanel>
                        </Border>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "DirectionalBinding.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="DirectionalBinding">
                      <Grid Background="White" Margin="12"
                            ColumnDefinitions="120, 120, *"
                            RowDefinitions="30, 30, 30, 30, 30, 30">
                        <Label Content="Total Income:"/>
                        <TextBlock Grid.Column="1" Text="{Binding total_income, Mode=OneTime}"/>
                        <TextBlock Grid.Column="2">OneTime Binding</TextBlock>

                        <Label Grid.Row="1" Content="Rent"/>
                        <TextBlock Grid.Row="1" Grid.Column="1"
                                   Text="{Binding rent, Mode=OneWay}" TargetUpdated="OnTargetUpdated"/>
                        <TextBlock Grid.Row="1" Grid.Column="2">OneWay Binding</TextBlock>

                        <Label Grid.Row="2" Content="Food"/>
                        <TextBox Grid.Row="2" Grid.Column="1"
                                 Text="{Binding food, UpdateSourceTrigger=PropertyChanged}"/>
                        <TextBlock Grid.Row="2" Grid.Column="2">TwoWay Binding, update on PropertyChanged</TextBlock>

                        <Label Grid.Row="3" Content="Miscellaneous"/>
                        <TextBox Grid.Row="3" Grid.Column="1" Text="{Binding misc}"/>
                        <TextBlock Grid.Row="3" Grid.Column="2">TwoWay Binding (TextBox default)</TextBlock>

                        <Label Grid.Row="4" Content="Savings"/>
                        <TextBlock Grid.Row="4" Grid.Column="1" Text="{Binding savings, StringFormat={}{0:F2}}"/>
                        <TextBlock Grid.Row="4" Grid.Column="2">OneWay Binding</TextBlock>

                        <Button x:Name="RaiseRent" Grid.Row="5" Width="110" Click="OnRentRaise">Raise the Rent!</Button>
                        <TextBlock Grid.Row="5" Grid.Column="1" Text="{Binding rent, StringFormat=rent is now {0:C0}}"/>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "StringFormat.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="StringFormat">
                      <StackPanel Background="White" Margin="8">
                        <TextBlock Margin="5,10,0,0" FontSize="20">Formatting a string on a Binding</TextBlock>
                        <TextBlock Margin="5" FontSize="14" Width="440" TextWrapping="Wrap">
                          This ListView contains a list of items for sale. The second column
                          displays the price formatted as a currency.
                        </TextBlock>
                        <ListView Width="420" HorizontalAlignment="Left" ItemsSource="{Binding items_for_sale}">
                          <ListView.View>
                            <GridView>
                              <GridViewColumn Header="Description" DisplayMemberBinding="{Binding description}" Width="2*"/>
                              <GridViewColumn Header="Price" DisplayMemberBinding="{Binding price, StringFormat=Now {0:c}!}" Width="*"/>
                            </GridView>
                          </ListView.View>
                        </ListView>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, wire_pages)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm {
        person_name: "Joe".into(),
        total_income: 5000.0,
        rent: 2000.0,
        food: "0".into(),
        misc: "0".into(),
        savings: 3000.0,
        items_for_sale: vec![
            SaleItem { description: "Snowboard".into(), price: 120.0 },
            SaleItem { description: "Fishing rod".into(), price: 27.13 },
            SaleItem { description: "Sailboat".into(), price: 8000.0 },
            SaleItem { description: "Kayak".into(), price: 249.99 },
        ],
        ..Default::default()
    });
    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="WPF-Samples on bevy_pf">
                  <DockPanel Background="White">
                    <StatusBar DockPanel.Dock="Bottom">
                      <StatusBarItem><TextBlock x:Name="Status" Text="sample: home"/></StatusBarItem>
                    </StatusBar>
                    <Frame Source="home.xaml"/>
                  </DockPanel>
                </Window>"##
        ),
        vm.clone(),
    );
    commands.insert_resource(VmHandle(vm));
}

/// Recreates each sample's code-behind as pages appear.
fn wire_pages(
    mut navigated: MessageReader<PfNavigated>,
    ui: PfQuery,
    mut texts: Query<&mut bevy::ui::widget::Text>,
    mut commands: Commands,
) {
    for nav in navigated.read() {
        if let Some(status) = ui.by_name("Status")
            && let Some(text_entity) = ui.first_text_in(status)
            && let Ok(mut text) = texts.get_mut(text_entity)
        {
            text.0 = format!(
                "sample: {}",
                nav.title.clone().unwrap_or_else(|| nav.source.clone())
            );
        }
        // DynamicLayout: HandleClick appended a TextBlock; here it appends
        // to a bound list, which the ItemsControl regenerates.
        if nav.source == "DynamicLayout.xaml"
            && let Some(button) = ui.by_name("Button1")
        {
            commands.entity(button).observe(
                |_: On<Pointer<Click>>, vm: Res<VmHandle>| {
                    vm.0.update(|m: &mut Vm| {
                        let n = m.lines.len() + 1;
                        m.lines.push(format!("You clicked the button! ({n})"));
                    });
                },
            );
        }
        // DirectionalBinding: OnRentRaise bumped Rent and recomputed savings.
        if nav.source == "DirectionalBinding.xaml"
            && let Some(button) = ui.by_name("RaiseRent")
        {
            commands.entity(button).observe(
                |_: On<Pointer<Click>>, vm: Res<VmHandle>| {
                    vm.0.update(|m: &mut Vm| {
                        m.rent = (m.rent * 1.05).round();
                        let food: f64 = m.food.trim().parse().unwrap_or(0.0);
                        let misc: f64 = m.misc.trim().parse().unwrap_or(0.0);
                        m.savings = m.total_income - m.rent - food - misc;
                    });
                },
            );
        }
    }
}
