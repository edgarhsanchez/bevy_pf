# Avalonia XAML in bevy_pf

bevy_pf implements the WPF/MAUI XAML dialect. Avalonia is a third dialect on
the same language, and this document records how much of it works, measured
rather than estimated.

Run the measurement yourself, in the companion repo
[bevy_pf_avalonia][companion]:

```
cargo run --bin conformance             # the report
cargo run --bin conformance -- --verbose
cargo run --example gallery             # see it render
```

It loads 69 `.axaml` files vendored **byte for byte** from
[AvaloniaUI/Avalonia.Samples][samples] (MIT). Nothing is edited to suit
bevy_pf — a sample that would need a tweak to load is a gap here, not a
problem with the sample. The suite lives in its own repo so this crate does
not redistribute someone else's sample files.

[samples]: https://github.com/AvaloniaUI/Avalonia.Samples
[companion]: https://github.com/edgarhsanchez/bevy_pf_avalonia

## AXAML is not a different language

The same XAML: the same XML object-graph markup, the same `x:` namespace
(literally the same Microsoft URI), the same markup-extension syntax with the
same `{}` and backslash escaping, the same property-element and
attached-property forms. The separate file extension exists because Visual
Studio applied WPF's designer and build actions to any `.xaml` file, which
produced spurious errors on Avalonia projects. Avalonia still accepts `.xaml`.

The evidence: bevy_pf's parser read 68 of the 69 sample files unmodified
before any Avalonia work was done. The one failure was `StringFormat='\{0\} %'`
— and that turned out to be a bug in bevy_pf's handling of backslash escapes,
which are standard XAML, not an Avalonia invention.

Every real difference is vocabulary or semantics.

## Vocabulary that maps onto something bevy_pf already had

| Avalonia | bevy_pf |
|---|---|
| `IsVisible` (bool) | shares the `Visibility` binding target, which already accepted booleans |
| `RequestedThemeVariant="Light\|Dark\|Default"` | the app theme; `Default` is `Unspecified` |
| `Spacing`, `RowSpacing`, `ColumnSpacing`, `ItemSpacing`, `LineSpacing` | the flex row/column gaps |
| `Grid ColumnDefinitions="Auto,*"` | the compact track-list form, already supported |
| `HorizontalContentAlignment` / `VerticalContentAlignment` | content-host alignment |
| `x:DataType`, `x:CompileBindings` | accepted and ignored — they are consumed by Avalonia's XAML *compiler* and mean nothing at runtime |

## Selector styling

Avalonia's `<Style Selector="...">` is the one deep difference: a second
styling model beside WPF's `Style`/`TargetType`/`x:Key`, driven by CSS-shaped
selectors and a `Classes` collection.

**It looks like CSS but is not CSS where it counts: Avalonia computes no
specificity.** Which style wins is decided by

1. a *bucket* — did the selector produce an **activator** (any class,
   pseudo-class or `[Prop=Val]` part; length and shape are irrelevant), and
   did it come from a `ControlTheme` or a `Styles` collection; then
2. within a bucket, **last attached wins**.

That is the rule `PfPropertyStore` already implements — highest tier wins, and
the last write to a tier is that tier's value — so selector styling needed no
priority arithmetic and no new tiers beyond two that were declared and never
written. A four-level structural selector is *not* activated; a one-character
class *is*. Under CSS that ordering would be reversed.

### Supported

- Selector grammar: type, `.class`, `#name`, `:pseudo-class`, descendant
  (space), child (`>`), `/template/`, `:nth-child()` / `:nth-last-child()`
  in every spelling (`2n+1`, `odd`, `even`, `-n+2`), `:not()`, `:is()`,
  `^` nesting, `[Prop=Val]`, comma alternation, and `prefix|Type`.
- `Styles` collections on any element (`<X.Styles>`), applying to that
  element's subtree.
- `Classes="a b c"`, including changes at runtime — the element restyles when
  a class is added and reverts when it is removed.
- Pseudo-classes ride the existing trigger runtime, because each already
  corresponds to a condition bevy_pf evaluated for WPF:

  | Avalonia | bevy_pf condition |
  |---|---|
  | `:pointerover` | `IsMouseOver` |
  | `:pressed` | `IsPressed` |
  | `:checked` / `:unchecked` | `IsChecked` |
  | `:disabled` / `:enabled` | `IsEnabled` |
  | `:focus` | `IsFocused` |
  | `:selected` | `IsSelected` |

  A `:pseudo` bevy_pf does not know falls back to a plain class test, so a
  control that sets it as a real class still works.

### Precedence

| Avalonia bucket | bevy_pf tier |
|---|---|
| `ControlTheme`, no activator | `ThemeStyle` (3) |
| `<Style Selector>`, no activator | `Style` (5) |
| `ControlTheme` nested style with an activator | `StyleTrigger` (7) |
| `<Style Selector>` with an activator | `ImplicitReference` (8) |
| local value (a XAML attribute) | `Local` (11) |

WPF styling is untouched: it writes at the same `Style` and `StyleTrigger`
tiers it always did, and `evaluate_triggers` clears per `(entity, tier)` pair,
so the two dialects coexist on one element without clobbering each other.

### Not yet

- `FluentTheme` and `StyleInclude` in a `Styles` collection.
- `ControlTheme` and `Theme=`.
- `<ResourceDictionary>` as a document root.
- `<Classes.foo>` bindings.
- `[Prop=Val]` selectors match nothing yet (they parse).

## Not bevy_pf's to fix

Some remaining warnings are not gaps and never close. The samples define
their own C# types, which have no counterpart in a Rust process:

- `services:DialogManager.Register` — an attached property from the sample's
  own code.
- `MainWindowViewModel`, `ViewLocator`, `SnowflakeGameViewModel` as resource
  types — the apps' view models.

Avalonia window chrome (`ExtendClientAreaToDecorationsHint`,
`TransparencyLevelHint`) has no bevy analogue either.

## Reading the report

It ranks complaints by **files affected**, not occurrences, and the two
disagree sharply. One sample re-templates a control and raises the same
`TemplateBinding` warning 59 times; by occurrence count that buries the gap
that 39 separate files actually needed. Only four files use `TemplateBinding`
at all.
