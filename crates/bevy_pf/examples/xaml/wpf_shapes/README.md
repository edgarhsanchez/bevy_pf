# Graphics/ShapeElements pages

Ported from [microsoft/WPF-Samples](https://github.com/microsoft/WPF-Samples)
(`Graphics/ShapeElements`), MIT license. Original XAML kept, with documented
adaptations marked by `<!-- Adaptation: ... -->` comments:

- App.xaml's graph-paper `DrawingBrush` resources -> solid brushes, inlined
  into each page's `Page.Resources` (the gallery hosts many samples, so
  nothing is app-scoped).
- `MyGridBorderStyle`'s class-qualified setters -> a `TargetType` style.
- MiterLimitExample's 3x `ScaleTransform` diagram copy -> shown at 1x.
- `x:Class` dropped; `WindowTitle` -> `Title`.

Loaded by `--example wpf_samples_gallery` via `include_xaml!` and served as
`shapes/<name>.xaml` routes inside the ShapeElements tab viewer.
