# bevy_pf_macros

Proc macros for [`bevy_pf`](https://crates.io/crates/bevy_pf): `xaml!{}` for
inline XAML and `include_xaml!()` for `.xaml` files, both **validated at
compile time** — a malformed document is a build error with a source
position, not a runtime surprise.

Depends only on [`bevy_pf_xaml`](https://crates.io/crates/bevy_pf_xaml), never
on Bevy, so it stays fast to compile.

You normally get these re-exported through `bevy_pf::prelude`:

```rust,ignore
commands.spawn_xaml(xaml!(
    r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   Margin="24" Spacing="8">
         <TextBlock Text="Hello from XAML" FontSize="28"/>
       </StackPanel>"#
));
```

## License

MIT OR Apache-2.0, at your option.
