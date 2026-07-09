//! Gallery of the WPF control set implemented so far: buttons, toggles,
//! checkboxes, radios, text input, slider, progress bar, list box, and the
//! layout panels, all styled with resources.
//!
//! Run with: `cargo run -p bevy_pf --example controls_gallery`

use bevy::prelude::*;
use bevy_pf::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_xaml(xaml!(
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="bevy_pf controls gallery">
              <Window.Resources>
                <Style x:Key="H1" TargetType="TextBlock">
                  <Setter Property="FontSize" Value="22"/>
                  <Setter Property="FontWeight" Value="SemiBold"/>
                  <Setter Property="Margin" Value="0,12,0,6"/>
                </Style>
                <Style TargetType="Border">
                  <Setter Property="Background" Value="#FAFAFA"/>
                  <Setter Property="BorderBrush" Value="#DDDDDD"/>
                  <Setter Property="BorderThickness" Value="1"/>
                  <Setter Property="CornerRadius" Value="8"/>
                  <Setter Property="Padding" Value="14"/>
                </Style>
              </Window.Resources>

              <ScrollViewer>
                <Grid Margin="24" ColumnDefinitions="*, 24, *">
                  <StackPanel>
                    <TextBlock Text="Buttons" Style="{StaticResource H1}"/>
                    <Border>
                      <StackPanel Orientation="Horizontal" Spacing="8">
                        <Button Content="Default"/>
                        <Button Content="Accent" Background="#0078D7" Foreground="White"/>
                        <ToggleButton Content="Toggle me"/>
                        <Button Content="Disabled" IsEnabled="False"/>
                      </StackPanel>
                    </Border>

                    <TextBlock Text="Selection" Style="{StaticResource H1}"/>
                    <Border>
                      <StackPanel Spacing="6">
                        <CheckBox Content="Enable sound" IsChecked="True"/>
                        <CheckBox Content="Fullscreen"/>
                        <Separator/>
                        <RadioButton GroupName="quality" Content="Low"/>
                        <RadioButton GroupName="quality" Content="Medium" IsChecked="True"/>
                        <RadioButton GroupName="quality" Content="High"/>
                      </StackPanel>
                    </Border>

                    <TextBlock Text="Text input" Style="{StaticResource H1}"/>
                    <Border>
                      <StackPanel Spacing="8">
                        <TextBox Text="Edit me — selection and clipboard work"/>
                        <TextBox MaxLength="12" Text="max 12 chars"/>
                      </StackPanel>
                    </Border>
                  </StackPanel>

                  <StackPanel Grid.Column="2">
                    <TextBlock Text="Range" Style="{StaticResource H1}"/>
                    <Border>
                      <StackPanel Spacing="10">
                        <Slider Minimum="0" Maximum="100" Value="30"/>
                        <ProgressBar Minimum="0" Maximum="100" Value="65"/>
                      </StackPanel>
                    </Border>

                    <TextBlock Text="ListBox" Style="{StaticResource H1}"/>
                    <Border>
                      <ListBox SelectedIndex="0" Height="120">
                        <ListBoxItem>Alpha</ListBoxItem>
                        <ListBoxItem>Beta</ListBoxItem>
                        <ListBoxItem>Gamma</ListBoxItem>
                        <ListBoxItem>Delta</ListBoxItem>
                      </ListBox>
                    </Border>

                    <TextBlock Text="Panels" Style="{StaticResource H1}"/>
                    <Border>
                      <DockPanel Height="140">
                        <Border DockPanel.Dock="Top" Background="#0078D7" Padding="4">
                          <TextBlock Text="Dock=Top" Foreground="White"/>
                        </Border>
                        <Border DockPanel.Dock="Left" Background="#E8E8E8" Padding="4">
                          <TextBlock Text="Left"/>
                        </Border>
                        <Border Background="White">
                          <UniformGrid Columns="3">
                            <TextBlock Text="1" HorizontalAlignment="Center"/>
                            <TextBlock Text="2" HorizontalAlignment="Center"/>
                            <TextBlock Text="3" HorizontalAlignment="Center"/>
                            <TextBlock Text="4" HorizontalAlignment="Center"/>
                            <TextBlock Text="5" HorizontalAlignment="Center"/>
                            <TextBlock Text="6" HorizontalAlignment="Center"/>
                          </UniformGrid>
                        </Border>
                      </DockPanel>
                    </Border>
                  </StackPanel>
                </Grid>
              </ScrollViewer>
            </Window>"##
    ));
}
