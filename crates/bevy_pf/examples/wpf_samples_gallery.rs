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
//!   the MultiBinding half is out of scope), DataTrigger (page-scoped
//!   trigger styles inside an ItemTemplate), CollectionBinding
//!   (selection-driven currency for the detail panel),
//!   PropertyChangeNotification (live bid prices from a Bevy system).
//! - Resources: DefiningResources (keyed brush + keyed styles, verbatim),
//!   MergedResources (sys: primitives, live dictionary merging).
//! - Elements: VisibiltyChanges (Visible/Hidden/Collapsed via observers),
//!   HeightProperties (selection-driven Height/Min/Max on a Rectangle).
//! - Sample Applications: CalculatorDemo (menus with a checkable item,
//!   tooltips, a Grid keypad, arithmetic + memory + paper tape in Rust).
//! - Graphics: ShapeElements — the 11-page tabbed shape viewer, original
//!   .xaml files included from examples/xaml/wpf_shapes/.
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

#[derive(Reflect, Default, Clone)]
struct Place {
    name: String,
    state: String,
}

#[derive(Reflect, Default, Clone)]
struct BidItem {
    bid_item_name: String,
    bid_item_price: f64,
}

#[derive(Reflect, Default, Clone)]
struct Friend {
    first_name: String,
    last_name: String,
    home_town: String,
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
    // DataTrigger's Places collection
    places: Vec<Place>,
    // CollectionBinding's People collection + current item (CollectionView
    // currency adapted as an explicit field).
    friends: Vec<Friend>,
    current: Friend,
    // PropertyChangeNotification's BidCollection
    bids: Vec<BidItem>,
}

#[derive(Resource)]
struct VmHandle(Bindable);

/// CalculatorDemo's code-behind state (MainWindow.cs), as a resource.
#[derive(Resource, Default)]
struct Calc {
    display: String,
    acc: f64,
    pending: Option<char>,
    fresh: bool,
    memory: Option<f64>,
    tape: Vec<String>,
}

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
                        <Hyperlink NavigateUri="DataTrigger.xaml" Margin="12,2,0,0">DataTrigger</Hyperlink>
                        <Hyperlink NavigateUri="CollectionBinding.xaml" Margin="12,2,0,0">CollectionBinding</Hyperlink>
                        <Hyperlink NavigateUri="PropertyChangeNotification.xaml" Margin="12,2,0,0">PropertyChangeNotification</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Resources" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="DefiningResources.xaml" Margin="12,2,0,0">DefiningResources</Hyperlink>
                        <Hyperlink NavigateUri="MergedResources.xaml" Margin="12,2,0,0">MergedResources</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Styles &amp; Templates" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="StylingAndTemplating.xaml" Margin="12,2,0,0">IntroToStylingAndTemplating</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Sample Applications" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="CalculatorDemo.xaml" Margin="12,2,0,0">CalculatorDemo</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Graphics" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="ShapeElements.xaml" Margin="12,2,0,0">ShapeElements (11 pages)</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Elements" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="VisibilityChanges.xaml" Margin="12,2,0,0">VisibiltyChanges</Hyperlink>
                        <Hyperlink NavigateUri="HeightProperties.xaml" Margin="12,2,0,0">HeightProperties</Hyperlink>
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
        .register_page(
            "DataTrigger.xaml",
            xaml!(
                // Deviations: local:Places resource -> `places` on the
                // DataContext; the DataType-implicit template -> explicit
                // ItemTemplate; the ListBoxItem implicit style -> keyed
                // styles on the template's own elements (generated item
                // wrappers are runtime entities, not XAML ListBoxItems).
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="DataTrigger">
                      <Page.Resources>
                        <Style x:Key="PlaceRow" TargetType="Canvas">
                          <Style.Triggers>
                            <MultiDataTrigger>
                              <MultiDataTrigger.Conditions>
                                <Condition Binding="{Binding name}" Value="Portland"/>
                                <Condition Binding="{Binding state}" Value="OR"/>
                              </MultiDataTrigger.Conditions>
                              <Setter Property="Background" Value="Cyan"/>
                            </MultiDataTrigger>
                          </Style.Triggers>
                        </Style>
                        <Style x:Key="PlaceState" TargetType="TextBlock">
                          <Style.Triggers>
                            <DataTrigger Binding="{Binding state}" Value="WA">
                              <Setter Property="Foreground" Value="Red"/>
                            </DataTrigger>
                          </Style.Triggers>
                        </Style>
                      </Page.Resources>
                      <StackPanel Background="White">
                        <TextBlock FontSize="18" Margin="5" FontWeight="Bold"
                                   HorizontalAlignment="Center">Data Trigger Sample</TextBlock>
                        <ListBox Width="180" HorizontalAlignment="Center" Background="Honeydew"
                                 ItemsSource="{Binding places}">
                          <ListBox.ItemTemplate>
                            <DataTemplate>
                              <Canvas Width="160" Height="20" Style="{StaticResource PlaceRow}">
                                <TextBlock FontSize="12" Width="130" Canvas.Left="0"
                                           Style="{StaticResource PlaceState}" Text="{Binding name}"/>
                                <TextBlock FontSize="12" Width="30" Canvas.Left="130"
                                           Style="{StaticResource PlaceState}" Text="{Binding state}"/>
                              </Canvas>
                            </DataTemplate>
                          </ListBox.ItemTemplate>
                        </ListBox>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "CollectionBinding.xaml",
            xaml!(
                // Deviations: local:People resource -> `friends` on the
                // DataContext; Person.ToString() -> DisplayMemberPath;
                // IsSynchronizedWithCurrentItem + keyed ContentTemplate ->
                // a `current` VM field kept in sync with the ListBox
                // selection by a system, with the detail template inlined.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="CollectionBinding">
                      <StackPanel Background="White">
                        <TextBlock FontSize="11" Margin="5,15,0,10" FontWeight="Bold">My Friends:</TextBlock>
                        <ListBox x:Name="FriendsList" Width="200" HorizontalAlignment="Left" Margin="5,0,0,0"
                                 ItemsSource="{Binding friends}" DisplayMemberPath="first_name"/>
                        <TextBlock FontSize="11" Margin="5,15,0,5" FontWeight="Bold">Information:</TextBlock>
                        <Border Width="300" Height="100" Margin="20" HorizontalAlignment="Left"
                                BorderBrush="Aqua" BorderThickness="1" Padding="8">
                          <Grid>
                            <Grid.RowDefinitions>
                              <RowDefinition/>
                              <RowDefinition/>
                              <RowDefinition/>
                            </Grid.RowDefinitions>
                            <Grid.ColumnDefinitions>
                              <ColumnDefinition/>
                              <ColumnDefinition/>
                            </Grid.ColumnDefinitions>
                            <TextBlock Grid.Row="0" Grid.Column="0" Text="First Name:"/>
                            <TextBlock Grid.Row="0" Grid.Column="1" Text="{Binding current.first_name}"/>
                            <TextBlock Grid.Row="1" Grid.Column="0" Text="Last Name:"/>
                            <TextBlock Grid.Row="1" Grid.Column="1" Text="{Binding current.last_name}"/>
                            <TextBlock Grid.Row="2" Grid.Column="0" Text="Home Town:"/>
                            <TextBlock Grid.Row="2" Grid.Column="1" Text="{Binding current.home_town}"/>
                          </Grid>
                        </Border>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "PropertyChangeNotification.xaml",
            xaml!(
                // Deviations: local:BidCollection -> `bids` on the
                // DataContext; the C# 2s timer -> a Bevy system mutating the
                // observable (raise_bids); keyed ItemTemplate resource ->
                // inline ItemTemplate.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="PropertyChangeNotification">
                      <DockPanel Width="350" Height="150" Background="White">
                        <TextBlock FontSize="18" Margin="5" FontWeight="Bold"
                                   DockPanel.Dock="Top">My Auction Tracker</TextBlock>
                        <ItemsControl x:Name="MyListBox" DockPanel.Dock="Top" Background="Silver"
                                      Width="315" Height="80" ItemsSource="{Binding bids}">
                          <ItemsControl.ItemTemplate>
                            <DataTemplate>
                              <Canvas Width="300" Height="20">
                                <TextBlock FontSize="14" Foreground="DarkSlateGray"
                                           Width="180" Canvas.Left="0" Text="{Binding bid_item_name}"/>
                                <TextBlock FontSize="14" Foreground="DarkSlateBlue"
                                           Text="$" Canvas.Left="180"/>
                                <TextBlock FontSize="14" Foreground="DarkSlateBlue" Width="80"
                                           Canvas.Left="190"
                                           Text="{Binding bid_item_price, StringFormat={}{0:F2}}"/>
                              </Canvas>
                            </DataTemplate>
                          </ItemsControl.ItemTemplate>
                        </ItemsControl>
                      </DockPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "DefiningResources.xaml",
            xaml!(
                // Deviations: none — the sample's resource surface (keyed
                // brush, keyed styles, DockPanel.Dock setters, StaticResource
                // references from text/button/shape) is fully supported.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="DefiningResources">
                      <Page.Resources>
                        <SolidColorBrush x:Key="MyBrush" Color="Gold"/>
                        <Style TargetType="Border" x:Key="PageBackground">
                          <Setter Property="Background" Value="Blue"/>
                        </Style>
                        <Style TargetType="TextBlock" x:Key="TitleText">
                          <Setter Property="Background" Value="Blue"/>
                          <Setter Property="DockPanel.Dock" Value="Top"/>
                          <Setter Property="FontSize" Value="18"/>
                          <Setter Property="Foreground" Value="#4E87D4"/>
                          <Setter Property="FontFamily" Value="Trebuchet MS"/>
                          <Setter Property="Margin" Value="0,40,10,10"/>
                        </Style>
                        <Style TargetType="TextBlock" x:Key="Label">
                          <Setter Property="DockPanel.Dock" Value="Right"/>
                          <Setter Property="FontSize" Value="8"/>
                          <Setter Property="Foreground" Value="{StaticResource MyBrush}"/>
                          <Setter Property="FontFamily" Value="Arial"/>
                          <Setter Property="FontWeight" Value="Bold"/>
                          <Setter Property="Margin" Value="0,3,10,0"/>
                        </Style>
                      </Page.Resources>
                      <StackPanel>
                        <Border Style="{StaticResource PageBackground}">
                          <DockPanel>
                            <TextBlock Style="{StaticResource TitleText}">Title</TextBlock>
                            <TextBlock Style="{StaticResource Label}">Label</TextBlock>
                            <TextBlock DockPanel.Dock="Top" HorizontalAlignment="Left" FontSize="36"
                                       Foreground="{StaticResource MyBrush}" Text="Text" Margin="20"/>
                            <Button DockPanel.Dock="Top" HorizontalAlignment="Left" Height="30"
                                    Background="{StaticResource MyBrush}" Margin="40">Button</Button>
                            <Ellipse DockPanel.Dock="Top" HorizontalAlignment="Left" Width="100" Height="100"
                                     Fill="{StaticResource MyBrush}" Margin="40"/>
                          </DockPanel>
                        </Border>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "VisibilityChanges.xaml",
            xaml!(
                // Deviation: Click code-behind -> Bevy observers (wired in
                // wire_pages).
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="VisibiltyChanges">
                      <Grid Background="White">
                        <Border BorderBrush="Black" BorderThickness="2" Background="White">
                          <DockPanel>
                            <TextBlock FontSize="20" FontWeight="Bold" DockPanel.Dock="Top" Margin="0,0,0,10">UIElement.Visibility Sample</TextBlock>
                            <TextBlock DockPanel.Dock="Top" Margin="0,0,0,10">Click the buttons below to manipulate the Visibility property of the TextBox below.</TextBlock>
                            <StackPanel DockPanel.Dock="Left">
                              <Button x:Name="btn1" Height="25">Visibility="Visible"</Button>
                              <Button x:Name="btn2" Height="25">Visibility="Hidden"</Button>
                              <Button x:Name="btn3" Height="25">Visibility="Collapsed"</Button>
                            </StackPanel>
                            <StackPanel HorizontalAlignment="Center">
                              <TextBox x:Name="tb1" Width="100" Height="50" Text="A TextBox"/>
                              <TextBlock x:Name="txt1" TextWrapping="Wrap" FontSize="14"/>
                            </StackPanel>
                          </DockPanel>
                        </Border>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "StylingAndTemplating.xaml",
            xaml!(
                // Deviations: the photo ListBox re-template (IsItemsHost +
                // ScrollViewer-in-template) and the MouseEnter Storyboards
                // are deferred features; this page keeps the sample's style
                // pipeline — implicit TextBlock style, BasedOn TitleText
                // with a gradient Foreground, and an implicit Button style
                // whose ControlTemplate + triggers restyle every button.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="StylingAndTemplating">
                      <Page.Resources>
                        <Style TargetType="TextBlock">
                          <Setter Property="HorizontalAlignment" Value="Center"/>
                          <Setter Property="FontSize" Value="14"/>
                        </Style>
                        <Style BasedOn="{StaticResource {x:Type TextBlock}}"
                               TargetType="TextBlock" x:Key="TitleText">
                          <Setter Property="FontSize" Value="26"/>
                          <Setter Property="Foreground" Value="#5BCCCC"/>
                        </Style>
                        <SolidColorBrush x:Key="Btn.Static" Color="#FFDDDDDD"/>
                        <SolidColorBrush x:Key="Btn.Hover" Color="#FFBEE6FD"/>
                        <SolidColorBrush x:Key="Btn.Pressed" Color="#FFC4E5F6"/>
                        <Style TargetType="Button">
                          <Setter Property="Background" Value="{StaticResource Btn.Static}"/>
                          <Setter Property="Padding" Value="10,4"/>
                          <Setter Property="Template">
                            <Setter.Value>
                              <ControlTemplate TargetType="Button">
                                <Border x:Name="chrome" CornerRadius="10" BorderThickness="2"
                                        BorderBrush="#FF3C7FB1"
                                        Background="{TemplateBinding Background}">
                                  <ContentPresenter Margin="{TemplateBinding Padding}"/>
                                </Border>
                                <ControlTemplate.Triggers>
                                  <Trigger Property="IsMouseOver" Value="True">
                                    <Setter TargetName="chrome" Property="Background" Value="{StaticResource Btn.Hover}"/>
                                  </Trigger>
                                  <Trigger Property="IsPressed" Value="True">
                                    <Setter TargetName="chrome" Property="Background" Value="{StaticResource Btn.Pressed}"/>
                                    <Setter TargetName="chrome" Property="BorderBrush" Value="#FF2C628B"/>
                                  </Trigger>
                                </ControlTemplate.Triggers>
                              </ControlTemplate>
                            </Setter.Value>
                          </Setter>
                        </Style>
                      </Page.Resources>
                      <StackPanel Background="White" Margin="16">
                        <TextBlock Style="{StaticResource TitleText}">Styling and Templating</TextBlock>
                        <TextBlock Margin="0,6,0,14" TextWrapping="Wrap" MaxWidth="480">Every TextBlock picks up the implicit style; the title extends it via BasedOn. Every Button below is re-templated by an implicit style: rounded chrome, TemplateBinding Background, and ControlTemplate.Triggers driving hover and press.</TextBlock>
                        <StackPanel Orientation="Horizontal" HorizontalAlignment="Center">
                          <Button Content="Templated" Margin="0,0,10,0"/>
                          <Button Content="Buttons" Margin="0,0,10,0"/>
                          <Button Content="Everywhere"/>
                        </StackPanel>
                        <TextBlock Margin="0,14,0,0" FontSize="11" Foreground="#FF8A8A8A">Hover and press them — the triggers write through the value store and revert to the template values.</TextBlock>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "MergedResources.xaml",
            xaml!(
                // Deviations: the two Source= dictionary files are inlined
                // (Source= file URIs are covered by resources_merged tests +
                // ExpenseIt); the code-behind's dictionary #3 file round-trip
                // becomes merge_application_resources, which BodyBrush's
                // DynamicResource re-resolves live.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          xmlns:sys="clr-namespace:System;assembly=MsCorLib"
                          Title="MergedResources" Background="{DynamicResource BodyBrush}">
                      <Page.Resources>
                        <ResourceDictionary>
                          <ResourceDictionary.MergedDictionaries>
                            <ResourceDictionary>
                              <sys:Double x:Key="MeaningOfLife">42</sys:Double>
                            </ResourceDictionary>
                            <ResourceDictionary>
                              <sys:String x:Key="HelloWorld">Hello, world</sys:String>
                            </ResourceDictionary>
                          </ResourceDictionary.MergedDictionaries>
                        </ResourceDictionary>
                      </Page.Resources>
                      <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                        <Button Content="{StaticResource MeaningOfLife}"/>
                        <Button Content="{StaticResource HelloWorld}"/>
                        <Button Name="NewD">Create or Load #3 Dictionary</Button>
                        <Button Name="Add2NewD">Add to #3 Dictionary</Button>
                        <TextBlock Name="RdStatus" Margin="0,10,0,0" FontSize="11"/>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "HeightProperties.xaml",
            xaml!(
                // Deviations: SelectionChanged code-behind -> a system
                // watching the three ListBox selections; ClipToBounds ->
                // the Canvas Overflow clip; value TextBlocks condensed.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="HeightProperties">
                      <Grid Background="White">
                        <Border BorderBrush="Black" BorderThickness="2" Background="White">
                          <StackPanel Margin="10">
                            <TextBlock FontSize="20">Height Properties Sample</TextBlock>
                            <TextBlock TextWrapping="Wrap" Margin="0,0,0,10">MinHeight takes precedence over MaxHeight, which takes precedence over Height. Use the ListBoxes to manipulate the Rectangle's Height properties.</TextBlock>
                            <Canvas Height="200" MinWidth="200" Background="#b0c4de" VerticalAlignment="Top" HorizontalAlignment="Center" Name="myCanvas" Margin="0,0,0,20">
                              <Rectangle Canvas.Top="50" Canvas.Left="50" Name="rect1" Fill="#4682b4" Height="100" Width="100"/>
                            </Canvas>
                            <StackPanel Orientation="Horizontal" HorizontalAlignment="Center">
                              <Button Name="ClipBtn" Margin="0,5,5,5">Canvas.ClipToBounds="True"</Button>
                              <Button Name="UnclipBtn" Margin="0,5,5,5">Canvas.ClipToBounds="False"</Button>
                            </StackPanel>
                            <StackPanel Orientation="Horizontal" HorizontalAlignment="Center" Margin="0,10,0,0">
                              <TextBlock Margin="10,0,0,0">Height:</TextBlock>
                              <ListBox Name="HeightList" Margin="10,0,0,0" Height="90" Width="60">
                                <ListBoxItem>25</ListBoxItem>
                                <ListBoxItem>50</ListBoxItem>
                                <ListBoxItem>100</ListBoxItem>
                                <ListBoxItem>150</ListBoxItem>
                                <ListBoxItem>200</ListBoxItem>
                              </ListBox>
                              <TextBlock Margin="20,0,0,0">MinHeight:</TextBlock>
                              <ListBox Name="MinHeightList" Margin="10,0,0,0" Height="90" Width="60">
                                <ListBoxItem>0</ListBoxItem>
                                <ListBoxItem>50</ListBoxItem>
                                <ListBoxItem>100</ListBoxItem>
                                <ListBoxItem>150</ListBoxItem>
                              </ListBox>
                              <TextBlock Margin="20,0,0,0">MaxHeight:</TextBlock>
                              <ListBox Name="MaxHeightList" Margin="10,0,0,0" Height="90" Width="60">
                                <ListBoxItem>50</ListBoxItem>
                                <ListBoxItem>100</ListBoxItem>
                                <ListBoxItem>150</ListBoxItem>
                                <ListBoxItem>200</ListBoxItem>
                              </ListBox>
                            </StackPanel>
                            <TextBlock Name="HeightStatus" Margin="0,10,0,0" FontSize="11"/>
                          </StackPanel>
                        </Border>
                      </Grid>
                    </Page>"##
            ),
        )
        .register_page(
            "CalculatorDemo.xaml",
            xaml!(
                // Deviations: local:MyTextBox (read-only display + paper
                // tape) -> bordered TextBlocks; Click= code-behind ->
                // observers wired by name in wire_pages; window Icon
                // dropped; File>Exit is native-only (toast on the web).
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="CalculatorDemo">
                      <DockPanel Name="MyPanel" Background="White">
                        <Menu DockPanel.Dock="Top" Height="26">
                          <MenuItem Header="File">
                            <MenuItem Name="CalcExit" Header="Exit"/>
                          </MenuItem>
                          <MenuItem Header="View">
                            <MenuItem Name="StandardMenu" IsCheckable="true" IsChecked="True" Header="Standard"/>
                          </MenuItem>
                          <MenuItem Header="Help">
                            <MenuItem Name="CalcAbout" Header="About"/>
                          </MenuItem>
                        </Menu>
                        <Grid Name="MyGrid" MaxWidth="640" MaxHeight="320">
                          <Grid.ColumnDefinitions>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                            <ColumnDefinition/>
                          </Grid.ColumnDefinitions>
                          <Grid.RowDefinitions>
                            <RowDefinition/>
                            <RowDefinition/>
                            <RowDefinition/>
                            <RowDefinition/>
                            <RowDefinition/>
                            <RowDefinition/>
                          </Grid.RowDefinitions>
                          <Button Name="B7" Grid.Column="4" Grid.Row="2">7</Button>
                          <Button Name="B8" Grid.Column="5" Grid.Row="2">8</Button>
                          <Button Name="B9" Grid.Column="6" Grid.Row="2">9</Button>
                          <Button Name="B4" Grid.Column="4" Grid.Row="3">4</Button>
                          <Button Name="B5" Grid.Column="5" Grid.Row="3">5</Button>
                          <Button Name="B6" Grid.Column="6" Grid.Row="3">6</Button>
                          <Button Name="B1" Grid.Column="4" Grid.Row="4">1</Button>
                          <Button Name="B2" Grid.Column="5" Grid.Row="4">2</Button>
                          <Button Name="B3" Grid.Column="6" Grid.Row="4">3</Button>
                          <Button Name="B0" Grid.Column="4" Grid.Row="5">0</Button>
                          <Button Name="BPeriod" Grid.Column="5" Grid.Row="5">.</Button>
                          <Button Name="BPM" Background="Darkgray" Grid.Column="6" Grid.Row="5">+/-</Button>
                          <Button Name="BDevide" Background="Darkgray" Grid.Column="7" Grid.Row="2">/</Button>
                          <Button Name="BMultiply" Background="Darkgray" Grid.Column="7" Grid.Row="3">*</Button>
                          <Button Name="BMinus" Background="Darkgray" Grid.Column="7" Grid.Row="4">-</Button>
                          <Button Name="BPlus" Background="Darkgray" Grid.Column="7" Grid.Row="5">+</Button>
                          <Button Name="BSqrt" Background="Darkgray" Grid.Column="8" Grid.Row="2" ToolTip="Usage: 'A Sqrt'">Sqrt</Button>
                          <Button Name="BPercent" Background="Darkgray" Grid.Column="8" Grid.Row="3" ToolTip="Usage: 'A % B ='">%</Button>
                          <Button Name="BOneOver" Background="Darkgray" Grid.Column="8" Grid.Row="4" ToolTip="Usage: 'A 1/X'">1/X</Button>
                          <Button Name="BEqual" Background="Darkgray" Grid.Column="8" Grid.Row="5">=</Button>
                          <Button Name="BC" Background="Darkgray" Grid.Column="8" Grid.Row="1" ToolTip="Clear All">C</Button>
                          <Button Name="BCE" Background="Darkgray" Grid.Column="7" Grid.Row="1" ToolTip="Clear Current Entry">CE</Button>
                          <Button Name="BMemClear" Background="Darkgray" Grid.Column="3" Grid.Row="2" ToolTip="Clear Memory">MC</Button>
                          <Button Name="BMemRecall" Background="Darkgray" Grid.Column="3" Grid.Row="3" ToolTip="Recall Memory">MR</Button>
                          <Button Name="BMemSave" Background="Darkgray" Grid.Column="3" Grid.Row="4" ToolTip="Store in Memory">MS</Button>
                          <Button Name="BMemPlus" Background="Darkgray" Grid.Column="3" Grid.Row="5" ToolTip="Add To Memory">M+</Button>
                          <TextBlock Name="BMemBox" Grid.Column="3" Grid.Row="1" Margin="10,17,10,17" Grid.ColumnSpan="2">Memory: [empty]</TextBlock>
                          <Border Grid.ColumnSpan="9" Height="30" Margin="5" Background="White"
                                  BorderBrush="#FF8A8A8A" BorderThickness="1" Padding="6,4">
                            <TextBlock Name="DisplayText" Text="0" HorizontalAlignment="Right" FontSize="15"/>
                          </Border>
                          <Border Name="PaperBox" Grid.Row="1" Grid.ColumnSpan="3" Grid.RowSpan="5" Margin="5"
                                  Background="#FFFDFDF2" BorderBrush="#FF8A8A8A" BorderThickness="1" Padding="6">
                            <ScrollViewer>
                              <TextBlock Name="PaperText" Text="paper tape" FontSize="11" TextWrapping="Wrap"/>
                            </ScrollViewer>
                          </Border>
                        </Grid>
                      </DockPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "ShapeElements.xaml",
            xaml!(
                // Graphics/ShapeElements' SampleViewer: a TabControl of
                // Frames, one per shape topic (the tiled ImageBrush window
                // background is dropped). Pages are the original .xaml files,
                // included via include_xaml! from examples/xaml/wpf_shapes/.
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="ShapeElements">
                      <DockPanel>
                        <TabControl Background="White">
                        <TabItem Header="Ellipse">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/EllipseExample.xaml"/>
                        </TabItem>
                        <TabItem Header="FillRule">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/FillRuleExample.xaml"/>
                        </TabItem>
                        <TabItem Header="LinecapsAndJoins">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/LinecapsAndJoinsExample.xaml"/>
                        </TabItem>
                        <TabItem Header="Line">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/LineExample.xaml"/>
                        </TabItem>
                        <TabItem Header="MiterLimit">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/MiterLimitExample.xaml"/>
                        </TabItem>
                        <TabItem Header="Path">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/PathExample.xaml"/>
                        </TabItem>
                        <TabItem Header="Polygon">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/PolygonExample.xaml"/>
                        </TabItem>
                        <TabItem Header="PolyLine">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/PolyLineExample.xaml"/>
                        </TabItem>
                        <TabItem Header="Rectangle">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/RectangleExample.xaml"/>
                        </TabItem>
                        <TabItem Header="Stretch">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/StretchExample.xaml"/>
                        </TabItem>
                        <TabItem Header="ShapeTypes">
                          <Frame Background="White" NavigationUIVisibility="Hidden" Source="shapes/ShapeTypes.xaml"/>
                        </TabItem>
                        </TabControl>
                      </DockPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "shapes/EllipseExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/EllipseExample.xaml"),
        )
        .register_page(
            "shapes/FillRuleExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/FillRuleExample.xaml"),
        )
        .register_page(
            "shapes/LinecapsAndJoinsExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/LinecapsAndJoinsExample.xaml"),
        )
        .register_page(
            "shapes/LineExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/LineExample.xaml"),
        )
        .register_page(
            "shapes/MiterLimitExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/MiterLimitExample.xaml"),
        )
        .register_page(
            "shapes/PathExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/PathExample.xaml"),
        )
        .register_page(
            "shapes/PolygonExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/PolygonExample.xaml"),
        )
        .register_page(
            "shapes/PolyLineExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/PolyLineExample.xaml"),
        )
        .register_page(
            "shapes/RectangleExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/RectangleExample.xaml"),
        )
        .register_page(
            "shapes/StretchExample.xaml",
            include_xaml!("examples/xaml/wpf_shapes/StretchExample.xaml"),
        )
        .register_page(
            "shapes/ShapeTypes.xaml",
            include_xaml!("examples/xaml/wpf_shapes/ShapeTypes.xaml"),
        )
        .add_systems(Startup, setup)
        .insert_resource(Calc {
            display: "0".into(),
            fresh: true,
            ..Default::default()
        })
        .add_systems(
            Update,
            (wire_pages, sync_collection_currency, raise_bids, sync_height_lists),
        )
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
        places: [
            ("Bellevue", "WA"), ("Gold Beach", "OR"), ("Kirkland", "WA"),
            ("Los Angeles", "CA"), ("Portland", "ME"), ("Portland", "OR"),
            ("Redmond", "WA"), ("San Diego", "CA"), ("San Francisco", "CA"),
            ("San Jose", "CA"), ("Seattle", "WA"),
        ]
        .into_iter()
        .map(|(name, state)| Place { name: name.into(), state: state.into() })
        .collect(),
        friends: [
            ("Michael", "Alexander", "Bellevue"),
            ("Jeff", "Hay", "Redmond"),
            ("Christina", "Lee", "Kirkland"),
            ("Samantha", "Smith", "Seattle"),
        ]
        .into_iter()
        .map(|(f, l, h)| Friend {
            first_name: f.into(),
            last_name: l.into(),
            home_town: h.into(),
        })
        .collect(),
        current: Friend {
            first_name: "Michael".into(),
            last_name: "Alexander".into(),
            home_town: "Bellevue".into(),
        },
        bids: vec![
            BidItem { bid_item_name: "Perseus Vase".into(), bid_item_price: 24.95 },
            BidItem { bid_item_name: "Hercules Statue".into(), bid_item_price: 16.05 },
            BidItem { bid_item_name: "Odysseus Painting".into(), bid_item_price: 100.0 },
        ],
        ..Default::default()
    });
    // PF_START_PAGE=<route> jumps straight to a sample (smoke-testing hook).
    let start = std::env::var("PF_START_PAGE").unwrap_or_else(|_| "home.xaml".into());
    let shell = format!(
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="WPF-Samples on bevy_pf">
              <DockPanel Background="White">
                <StatusBar DockPanel.Dock="Bottom">
                  <StatusBarItem><TextBlock x:Name="Status" Text="sample: home"/></StatusBarItem>
                </StatusBar>
                <Frame Source="{start}"/>
              </DockPanel>
            </Window>"##
    );
    commands.spawn_xaml_bound(
        XamlScene::parse(shell).expect("shell template is valid XAML"),
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
        // MergedResources: dictionary #3 creation + BodyBrush=Green add.
        if nav.source == "MergedResources.xaml"
            && let (Some(new_d), Some(add_d), Some(status)) =
                (ui.by_name("NewD"), ui.by_name("Add2NewD"), ui.by_name("RdStatus"))
        {
            commands.entity(new_d).observe(
                move |_: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        bevy_pf::instantiate::set_text(
                            world,
                            status,
                            "dictionary #3 ready (empty)".into(),
                        );
                    });
                },
            );
            commands.entity(add_d).observe(
                move |_: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        let mut entries = bevy_pf::resources::ResourceDictionary::new();
                        entries.insert(
                            bevy_pf::resources::ResourceKey::Explicit("BodyBrush".into()),
                            bevy_pf::resources::PfValue::Brush(
                                bevy_pf::xaml_ast::value::PfBrush::Solid(
                                    bevy_pf::xaml_ast::value::PfColor::rgb(0x00, 0x80, 0x00),
                                ),
                            ),
                        );
                        bevy_pf::merge_application_resources(world, entries);
                        bevy_pf::instantiate::set_text(
                            world,
                            status,
                            "BodyBrush=Green added; DynamicResource re-resolved the Background".into(),
                        );
                    });
                },
            );
        }
        // HeightProperties: ClipToBounds buttons (the lists have a system).
        if nav.source == "HeightProperties.xaml"
            && let Some(canvas) = ui.by_name("myCanvas")
        {
            for (name, clip) in [("ClipBtn", true), ("UnclipBtn", false)] {
                let Some(button) = ui.by_name(name) else { continue };
                commands.entity(button).observe(
                    move |_: On<Pointer<Click>>, mut nodes: Query<&mut Node>| {
                        if let Ok(mut node) = nodes.get_mut(canvas) {
                            node.overflow = if clip {
                                bevy::ui::Overflow::clip()
                            } else {
                                bevy::ui::Overflow::visible()
                            };
                        }
                    },
                );
            }
        }
        // CalculatorDemo: DigitBtn_Click / OperBtn_Click / menu handlers.
        if nav.source == "CalculatorDemo.xaml" {
            wire_calculator(&ui, &mut commands);
        }
        // VisibiltyChanges: ContentVis/ContentHid/ContentCol code-behind ->
        // the property store's Visibility target (same tier as the XAML attr).
        if nav.source == "VisibilityChanges.xaml"
            && let (Some(target), Some(status)) = (ui.by_name("tb1"), ui.by_name("txt1"))
        {
            for (button, state, note) in [
                ("btn1", "Visible", "Visibility is now Visible."),
                ("btn2", "Hidden", "Visibility is now Hidden. It occupies layout space."),
                ("btn3", "Collapsed", "Visibility is now Collapsed. It occupies no layout space."),
            ] {
                let Some(button) = ui.by_name(button) else { continue };
                commands.entity(button).observe(
                    move |_: On<Pointer<Click>>, mut commands: Commands| {
                        commands.queue(move |world: &mut World| {
                            bevy_pf::provider::set_local(
                                world,
                                target,
                                bevy_pf::provider::PropertyTarget::Visibility,
                                bevy_pf::resources::PfValue::String(state.to_string()),
                            );
                            bevy_pf::instantiate::set_text(world, status, note.to_string());
                        });
                    },
                );
            }
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

/// CollectionBinding: WPF's `IsSynchronizedWithCurrentItem` keeps the detail
/// ContentControl on the collection's current item. Here the ListBox
/// selection drives an explicit `current` field on the view-model.
fn sync_collection_currency(
    lists: Query<
        (&bevy_pf::components::PfListBox, &Children),
        Changed<bevy_pf::components::PfListBox>,
    >,
    ui: PfQuery,
    vm: Res<VmHandle>,
) {
    let Some(list_entity) = ui.by_name("FriendsList") else {
        return;
    };
    let Ok((list, children)) = lists.get(list_entity) else {
        return;
    };
    let Some(selected) = list.selected else {
        return;
    };
    let Some(index) = children.iter().position(|c| c == selected) else {
        return;
    };
    vm.0.update(|m: &mut Vm| {
        if let Some(f) = m.friends.get(index) {
            m.current = f.clone();
        }
    });
}

/// PropertyChangeNotification: the sample's 2-second timer, as a Bevy system.
/// Mutating the observable is all it takes — bound rows re-render.
fn raise_bids(time: Res<Time>, mut elapsed: Local<f32>, vm: Res<VmHandle>) {
    *elapsed += time.delta_secs();
    if *elapsed < 2.0 {
        return;
    }
    *elapsed = 0.0;
    vm.0.update(|m: &mut Vm| {
        if m.bids.len() == 3 {
            m.bids[0].bid_item_price += 1.25;
            m.bids[1].bid_item_price += 2.45;
            m.bids[2].bid_item_price += 10.55;
        }
    });
}

/// CalculatorDemo: every Click= handler from MainWindow.cs as observers.
fn wire_calculator(ui: &PfQuery, commands: &mut Commands) {
    let Some(display) = ui.by_name("DisplayText") else {
        return;
    };
    let (Some(paper), Some(mem_box)) = (ui.by_name("PaperText"), ui.by_name("BMemBox")) else {
        return;
    };

    // Reset state for a fresh visit (pages re-instantiate per navigation).
    commands.queue(move |world: &mut World| {
        *world.resource_mut::<Calc>() = Calc {
            display: "0".into(),
            fresh: true,
            ..Default::default()
        };
        calc_render(world, display, paper, mem_box);
    });

    let digits = [
        ("B0", '0'), ("B1", '1'), ("B2", '2'), ("B3", '3'), ("B4", '4'),
        ("B5", '5'), ("B6", '6'), ("B7", '7'), ("B8", '8'), ("B9", '9'),
        ("BPeriod", '.'),
    ];
    for (name, digit) in digits {
        let Some(button) = ui.by_name(name) else { continue };
        commands.entity(button).observe(
            move |_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    let calc = &mut *world.resource_mut::<Calc>();
                    if calc.fresh {
                        calc.display = if digit == '.' { "0.".into() } else { digit.to_string() };
                        calc.fresh = false;
                    } else if digit != '.' || !calc.display.contains('.') {
                        calc.display.push(digit);
                    }
                    calc_render(world, display, paper, mem_box);
                });
            },
        );
    }

    let opers = [
        ("BPlus", "+"), ("BMinus", "-"), ("BMultiply", "*"), ("BDevide", "/"),
        ("BPercent", "%"), ("BEqual", "="), ("BPM", "±"), ("BSqrt", "sqrt"),
        ("BOneOver", "1/x"), ("BC", "C"), ("BCE", "CE"),
        ("BMemClear", "MC"), ("BMemRecall", "MR"), ("BMemSave", "MS"), ("BMemPlus", "M+"),
    ];
    for (name, op) in opers {
        let Some(button) = ui.by_name(name) else { continue };
        commands.entity(button).observe(
            move |_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    calc_operate(world, op);
                    calc_render(world, display, paper, mem_box);
                });
            },
        );
    }

    // Menu: Exit (native only), Standard view toggle, About dialog.
    if let Some(exit) = ui.by_name("CalcExit") {
        commands.entity(exit).observe(
            |_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    #[cfg(not(target_arch = "wasm32"))]
                    world.write_message(bevy::app::AppExit::Success);
                    #[cfg(target_arch = "wasm32")]
                    bevy_pf::toast::show(world, "Exit closes the native window; on the web, just close the tab.");
                });
            },
        );
    }
    if let (Some(standard), Some(tape)) = (ui.by_name("StandardMenu"), ui.by_name("PaperBox")) {
        commands.entity(standard).observe(
            move |_: On<Pointer<Click>>, mut nodes: Query<&mut Node>| {
                if let Ok(mut node) = nodes.get_mut(tape) {
                    node.display = match node.display {
                        Display::None => Display::Flex,
                        _ => Display::None,
                    };
                }
            },
        );
    }
    if let Some(about) = ui.by_name("CalcAbout") {
        commands.entity(about).observe(
            |_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    bevy_pf::dialog::show_message(
                        world,
                        "About Calculator",
                        "The WPF-Samples CalculatorDemo, running on bevy_pf. \
                         Menus, tooltips, a Grid keypad, and a checkable View menu \
                         — arithmetic lives in Bevy observers.",
                        &["OK"],
                    );
                });
            },
        );
    }
}

fn calc_operate(world: &mut World, op: &str) {
    let calc = &mut *world.resource_mut::<Calc>();
    let entry: f64 = calc.display.parse().unwrap_or(0.0);
    match op {
        "C" => {
            let memory = calc.memory;
            *calc = Calc { display: "0".into(), fresh: true, memory, ..Default::default() };
        }
        "CE" => {
            calc.display = "0".into();
            calc.fresh = true;
        }
        "±" => {
            if calc.display.starts_with('-') {
                calc.display.remove(0);
            } else if entry != 0.0 {
                calc.display.insert(0, '-');
            }
        }
        "sqrt" => {
            calc.tape.push(format!("sqrt({}) = {}", fmt_num(entry), fmt_num(entry.sqrt())));
            calc.display = fmt_num(entry.sqrt());
            calc.fresh = true;
        }
        "1/x" => {
            let r = if entry == 0.0 { f64::NAN } else { 1.0 / entry };
            calc.tape.push(format!("1/{} = {}", fmt_num(entry), fmt_num(r)));
            calc.display = fmt_num(r);
            calc.fresh = true;
        }
        "MC" => calc.memory = None,
        "MR" => {
            if let Some(m) = calc.memory {
                calc.display = fmt_num(m);
                calc.fresh = true;
            }
        }
        "MS" => calc.memory = Some(entry),
        "M+" => calc.memory = Some(calc.memory.unwrap_or(0.0) + entry),
        // +, -, *, /, %, = — apply any pending operator, then queue this one.
        _ => {
            let result = match calc.pending {
                Some('+') => calc.acc + entry,
                Some('-') => calc.acc - entry,
                Some('*') => calc.acc * entry,
                Some('/') => {
                    if entry == 0.0 { f64::NAN } else { calc.acc / entry }
                }
                Some('%') => calc.acc * entry / 100.0,
                _ => entry,
            };
            if let Some(prev) = calc.pending {
                calc.tape.push(format!(
                    "{} {} {} = {}",
                    fmt_num(calc.acc), prev, fmt_num(entry), fmt_num(result)
                ));
            }
            calc.acc = result;
            calc.display = fmt_num(result);
            calc.pending = op.chars().next().filter(|c| *c != '=');
            calc.fresh = true;
        }
    }
}

fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        "Error".into()
    } else if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn calc_render(world: &mut World, display: Entity, paper: Entity, mem_box: Entity) {
    let calc = world.resource::<Calc>();
    let display_text = calc.display.clone();
    let tape = if calc.tape.is_empty() {
        "paper tape".to_string()
    } else {
        calc.tape.join("\n")
    };
    let mem = match calc.memory {
        Some(m) => format!("Memory: {}", fmt_num(m)),
        None => "Memory: [empty]".to_string(),
    };
    bevy_pf::instantiate::set_text(world, display, display_text);
    bevy_pf::instantiate::set_text(world, paper, tape);
    bevy_pf::instantiate::set_text(world, mem_box, mem);
}

/// HeightProperties: the three SelectionChanged handlers — each ListBox
/// selection writes one height field on the Rectangle.
fn sync_height_lists(
    lists: Query<
        (&bevy_pf::components::PfListBox, &bevy_pf::PfName, &Children),
        Changed<bevy_pf::components::PfListBox>,
    >,
    ui: PfQuery,
    mut nodes: Query<&mut Node>,
    texts: Query<&bevy::ui::widget::Text>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    let Some(rect) = ui.by_name("rect1") else {
        return;
    };
    for (list, name, children) in &lists {
        let field = match name.0.as_str() {
            "HeightList" => 0,
            "MinHeightList" => 1,
            "MaxHeightList" => 2,
            _ => continue,
        };
        let Some(selected) = list.selected else { continue };
        // The item's label text is the pixel value.
        let mut value = None;
        let mut stack: Vec<Entity> = vec![selected];
        while let Some(e) = stack.pop() {
            if let Ok(t) = texts.get(e)
                && let Ok(v) = t.0.trim().parse::<f32>()
            {
                value = Some(v);
                break;
            }
            if let Ok(c) = children_q.get(e) {
                stack.extend(c.iter());
            }
        }
        let Some(v) = value else { continue };
        if let Ok(mut node) = nodes.get_mut(rect) {
            match field {
                0 => node.height = Val::Px(v),
                1 => node.min_height = Val::Px(v),
                _ => node.max_height = Val::Px(v),
            }
        }
        let _ = children;
        if let Some(status) = ui.by_name("HeightStatus") {
            let label = ["Height", "MinHeight", "MaxHeight"][field];
            commands.queue(move |world: &mut World| {
                bevy_pf::instantiate::set_text(
                    world,
                    status,
                    format!("{label} set to {v}px (MinHeight > MaxHeight > Height)"),
                );
            });
        }
    }
}
