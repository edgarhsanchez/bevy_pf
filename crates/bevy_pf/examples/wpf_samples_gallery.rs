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
//! - Resources: DefiningResources (keyed brush + keyed styles, verbatim).
//! - Elements: VisibiltyChanges (Visible/Hidden/Collapsed via observers).
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
                        <TextBlock FontWeight="Bold" Text="Graphics" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="ShapeElements.xaml" Margin="12,2,0,0">ShapeElements (11 pages)</Hyperlink>
                        <TextBlock FontWeight="Bold" Text="Elements" Margin="0,8,0,4"/>
                        <Hyperlink NavigateUri="VisibilityChanges.xaml" Margin="12,2,0,0">VisibiltyChanges</Hyperlink>
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
        .add_systems(Update, (wire_pages, sync_collection_currency, raise_bids))
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
