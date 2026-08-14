# bevy_pf_xaml

The XAML parser and core value types behind [`bevy_pf`](https://crates.io/crates/bevy_pf) —
with **no Bevy dependency**, so it is usable from proc macros, build scripts,
and ordinary Rust programs.

WPF semantics are the specification, not an inspiration: namespaces,
property-element syntax (`<Button.Content>`), attached properties
(`Grid.Row="1"`), markup extensions (`{StaticResource}`, `{DynamicResource}`,
`{x:Null}`, nested and `{}`-escaped), `x:Name`/`x:Key`/`x:Class`,
`mc:Ignorable` design-time skipping, and WPF's whitespace rules.

The type converters match WPF byte-for-byte where it counts: all 141 named
colors, `#RGB`/`#ARGB`/`#RRGGBB`/`#AARRGGBB`, `sc#`, `Thickness`,
`CornerRadius`, `GridLength` (`Auto`/`*`/`2.5*`/px), `Point`, `Size`, `Rect`,
`Duration`, and `FontWeight` — all case-insensitive, like WPF.

That fidelity is audited rather than assumed: `docs/wpf-conformance-notes.md`
in the [repository](https://github.com/edgarhsanchez/bevy_pf) records what
matches the `dotnet/wpf` reference source and every accepted deviation, with
reasons.

```toml
bevy_pf_xaml = "0.1"
```

## License

MIT OR Apache-2.0, at your option.
