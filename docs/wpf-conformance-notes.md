# WPF Conformance Notes

Synthesized 2026-07-08 from a source-level audit of `dotnet/wpf@main` (MIT) against bevy_pf.
WPF paths below are relative to `src/Microsoft.DotNet.Wpf/src/` unless noted; framework-element
paths abbreviate `PresentationFramework/System/Windows/` as `PF/`. Our paths are repo-relative.

Compat contract (adopted from the roadmap): **any deviation from WPF is a bug** unless it is
listed here as an *accepted deviation* with rationale. Errors are non-fatal — every gap must
degrade to a warning, never a panic.

---

## 1. Verified conformant

Behaviors confirmed to match WPF by reading the reference source.

### Layout (`crates/bevy_pf/src/instantiate.rs`, `convert.rs`)

- **Grid.Row/Column out-of-range clamping** to the last track (`instantiate.rs:1277-1278` = `PF/Controls/Grid.cs:953-956`).
- **Default single implicit `*` row/column** when no definitions given (`instantiate.rs:1221-1227` = `Grid.cs:1045`); un-annotated children overlap at (0,0).
- **Canvas Left-beats-Right / Top-beats-Bottom** (`instantiate.rs:1298-1308` = `PF/Controls/Canvas.cs:291-293`).
- **DockPanel default `Dock.Left`** and **LastChildFill ignores the last child's Dock** (`instantiate.rs:2185,2140` = `PF/Controls/DockPanel.cs:173,265,284`).
- **StackPanel main-axis alignment is a no-op** — WPF arranges each child at exactly its desired main-axis size (`Stack.cs:745-757`); our `apply_h_alignment`/`apply_v_alignment` (`instantiate.rs:1176-1209`) match.
- **Visibility Hidden vs Collapsed** semantics (`convert.rs:83-91`).
- **Width/Min/Max clamp order — "Min beats Max"** (CSS shares WPF's MinMax semantics, `PF/FrameworkElement.cs:4036-4065`).

### Markup-extension parser (`crates/bevy_pf_xaml/src/markup.rs`)

- **`{}` escape prefix** for attribute literals, stripped once, incl. mid-value `StringFormat={}{0:00}` (`XamlText.cs:51-63`, `MeScanner.cs:242-247`).
- **Backslash escaping** of `{ } , = \` in unquoted values and inside quoted strings, with backslash removal (MeScanner rule 1; `ReadString` 359-364 + `RemoveEscapes`).
- **Trim unquoted, don't trim quoted** values (`MeScanner.cs:515-520`) — modulo deviation P11 (whitespace set).
- **Balanced literal braces in unquoted strings** (`Path=a{b}c`; v3-compat `braceCount`, `MeScanner.cs:349,433-448`) — modulo deviation P7.
- **Nested extensions** via unquoted `{...}` in positional and named positions, arbitrarily deep (`MePullParser.cs` P_Value 335-374).
- **Element text content is never ME-parsed** — `LooksLikeAMarkupExtension` is only consulted for attributes (`XamlPullParser.cs:848`).

### Value converters (`crates/bevy_pf_xaml/src/value.rs`)

- **Thickness 1/2/4-value forms**, comma and/or whitespace separators, `Auto` → NaN per component (`ThicknessConverter.cs:186-212`).
- **GridLength core forms**: case-insensitive `Auto`, bare `*` → `Star(1)`, `N*`, plain number → Pixel, whitespace trimmed (`XamlGridLengthSerializer.cs:179-242`).
- **Invariant-culture numeric parsing with `,` list separator** — WPF XAML/BAML conversion uses InvariantEnglishUS, so our hardcoded `.` decimal + `,`/space separators match; the `;`-separator regime only arises in non-invariant runtime API calls we don't expose.
- **Hex colors**: exactly `#RGB`/`#ARGB`/`#RRGGBB`/`#AARRGGBB`, nibble×17 expansion, input trimmed (`Parsers.cs:56-75`, `Knowncolors.cs:223-236`).
- **Named colors**: all 141, case-insensitive, `Transparent = #00FFFFFF` — byte-for-byte match.
- **sc# arity/order**: 3 values → alpha 1.0; 4 values → A,R,G,B (`Parsers.cs:150-172`) — modulo deviations P17/P18.
- **FontWeight**: full name set (Regular=400, DemiBold=600, Heavy=900, ExtraBlack/UltraBlack=950), case-insensitive, numeric 1-999 with leading-sign tolerance (`FontWeights.cs:97-210`).
- **Enum values case-insensitive** (matches .NET `Enum.Parse(..., ignoreCase: true)`).
- **CornerRadius 1-or-4 arity** (`CornerRadiusConverter.cs:154-185`) — modulo deviations P14/P16.

### Whitespace (`crates/bevy_pf_xaml/src/parser.rs`)

- **Edge trimming + interior collapse** for non-preserved element text, keeping the trailing space in `Hello <Bold>World</Bold>!` (`XamlText.cs` Paste 141-158, CollapseWhitespace 201-251).

---

## 2. Known deviations

Severity legend: **VIS** = visible layout/behavior difference in common XAML; **USE** = visible when the feature is used; **EDGE** = edge case; **COS** = cosmetic.

### 2.1 Layout deviations (L#)

| # | Behavior | WPF source | Our behavior | Sev | Fix strategy |
|---|---|---|---|---|---|
| L1 | Star tracks are strict proportions; children measured *at* resolved star size, content clips (star min ≠ content min) | `Grid.cs:2038-2065`; clip at `FrameworkElement.cs:4441-4451` | `GridTrack::fr(f)` = `minmax(auto,1fr)` — star tracks grow past their share to fit content (`convert.rs:30-36`) | **VIS** | Emit `minmax(px(0), fr(f))`. Verify the infinite-constraint case (WPF: Star→Auto, `Grid.cs:1150-1154`); special-case if taffy differs. **Fix now.** |
| L2 | Row/ColumnDefinition Min/Max clamp star shares and redistribute surplus by max-discrepancy | `Grid.cs:1859-1964` | Min/Max attributes on definitions silently dropped (`instantiate.rs:1231-1264`) | USE | Min: `minmax(px(min), fr(f))`. Max on Pixel/Auto: `minmax(auto, px(max))`. Max on Star is inexpressible in CSS grid — **accepted deviation** short-term; warn + document; custom measure later. |
| L3 | Span clamped to grid edge: `span = min(span, count − index)` | `Grid.cs:962-966` | Raw span passed through → taffy creates phantom implicit tracks (`instantiate.rs:1275-1281`) | EDGE | One-line clamp before `GridPlacement::start_span`. **Fix now.** |
| L4 | Span min-size distribution keeps Auto tracks tight, grows Pixel/Star toward preferred/max first | `Grid.cs:1533-1560` | CSS grid distributes to intrinsic (auto) tracks, never grows fixed ones | EDGE | **Accepted deviation** — fixing requires replacing taffy's grid sizing. |
| L5 | Docked child's slot spans full remaining cross extent; alignment resolves inside it | `DockPanel.cs:278-308` + `FrameworkElement.cs:4611-4619` | Children spawned as `ParentKind::FlexColumn` before dock direction known → `HorizontalAlignment` maps to wrong axis in Left/Right wrappers, `VerticalAlignment` dropped (`instantiate.rs:1510-1527,2136-2213`) | **VIS** | Defer alignment for DockPanel children; apply in `build_dock_chain` once wrapper axis is known (Left/Right: V-align→`align_self`; Top/Bottom mirrored). |
| L6 | StackPanel children arranged at exactly DesiredSize on the stacking axis; they overflow, never shrink | `Stack.cs:559-570,745-757` | Bevy `Node` default `flex_shrink: 1.0` lets children shrink to fit | **VIS** | Set `flex_shrink: 0.0` on children of StackPanel/WrapPanel and on the DockPanel dock axis. **Fix now.** |
| L7 | WrapPanel: every child's slot cross = line extent (default-Stretch children fill the line); lines pack at natural size from the start | `WrapPanel.cs:342-346,254-255` | `align_items: FlexStart` + default `align_content` (stretch) (`instantiate.rs:465-471`) | **VIS** | `align_items: Stretch`, `align_content: FlexStart`. Two-line change; batch with L6. |
| L8 | WrapPanel `ItemWidth`/`ItemHeight` slot every child at ItemU and constrain measure | `WrapPanel.cs:219-221,341` | Unsupported (warning, `instantiate.rs:1167-1171`) | USE | Wrap each child in a fixed ItemWidth×ItemHeight single-cell-grid slot so alignment resolves inside it. |
| L9 | Canvas children measured against (+inf, +inf) — text never wraps | `Canvas.cs:255-266` | Taffy absolute nodes shrink-to-fit within the containing block → text can wrap (`instantiate.rs:1284-1309`) | EDGE | **Accepted deviation** for now; long-term `max-content` width via custom measure. |
| L10 | "Parent wins" desired-size clipping; child never arranged below desired; Stretch degrades to Top/Left on overflow | `FrameworkElement.cs:4441-4451,4597-4607,4814-4824` | Taffy overflow behavior varies per container (flex shrinks, grid overflows from alignment position) | EDGE/COS | **Accepted deviation** — faithful fix requires a WPF-style arrange pass. |
| L11 | Grid cyclic Auto/Star topology fixpoint loop (`c_layoutLoopMaxCount` retries) | `Grid.cs:644-676,492-531` | Taffy single-pass spec sizing resolves these differently | EDGE | **Accepted deviation.** |
| L12 | Arrange-time star re-resolution + per-track rounding-error distribution | `Grid.cs:2354+,2595+` | Taffy's own rounding | COS | **Accepted deviation** — 1px seams at fractional DPI possible; rely on taffy rounding. |

### 2.2 Parser/converter deviations (P#)

See §3 for the cheap fixes (P1-P17, P20-P21 are all "fix now"). Accepted deviations in this category:

| # | Behavior | WPF source | Our behavior | Sev | Decision |
|---|---|---|---|---|---|
| P18 | `sc#` components are scRGB (linear); byte channels get the linear→sRGB transfer | `Parsers.cs:167,171` (`Color.FromScRgb`) | Treated as sRGB floats: `sc#0.5,...` → 128, WPF ≈ 188 (`value.rs:220-233`) | USE | Fix if cheap: apply `c ≤ 0.0031308 ? 12.92c : 1.055c^(1/2.4) − 0.055`; otherwise **accepted deviation**, documented. |
| P19 | `ContextColor <profileUri> <a,ch...>` ICC syntax | `Parsers.cs:80-137` | Unsupported → "unknown color name" | EDGE | **Accepted deviation** — needs ICC infrastructure; improve the error message to name the syntax. |
| P22 | Single newline between East-Asian code points collapses to nothing, not a space | `XamlText.cs:225-245,268-380` | Always emits a space (`parser.rs:237-264`) | EDGE | **Accepted deviation** (optional CJK-range check later). |
| — | Bare `"px"` → `0.0` in LengthConverter (quirk); bare `"px"` for GridLength throws | `LengthConverter.cs:208-209`; `XamlGridLengthSerializer.cs:229-242` | n/a until P12 lands | EDGE | Match when implementing P12; documentation note. |

Runtime note (not a parser change): in curly syntax WPF resolves `{Foo ...}` by trying
`FooExtension` before `Foo` (`MeScanner.cs:287-312`). Our runtime ME-name lookup should mirror
that if custom extensions ever register with an `Extension` suffix.

Additional accepted deviations adopted while landing the §3 fixes (2026-07-09):

| Behavior | WPF | Ours | Rationale |
|---|---|---|---|
| Bracket characters in barewords | Type-driven: `MarkupExtensionBracketCharacters` on the extension's ctor params declares pairs (Binding declares `[`/`]`; DRTs declare `(`/`)`, `$`/`^`) | Universal `[`...`]` and `(`...`)` grouping (quotes/`,`/`=` literal inside); custom pairs like `$`...`^` unsupported | We have no type registry at parse time; `[]`+`()` covers Binding indexers and parenthesized attached paths, the only shipping uses. The bracket-character DRT is skipped in sweeps. |
| `,`/`=` inside literal braces of a `{}`-escaped value (`StringFormat={}{0:#,.##} K`) | Unverified against MeScanner (escape handling path) | Allowed — the escaped remainder is literal text | Required by real-world .NET numeric format strings (NoesisGUI Scoreboard sample); strict P7 still applies to non-escaped barewords. |

Resource-system deviations recorded while landing MergedDictionaries/DynamicResource
(2026-07-09, adversarial review confirmed the rest were fixed — see
`crates/bevy_pf/tests/resources_merged.rs` regression tests):

| Behavior | WPF | Ours | Status |
|---|---|---|---|
| Theme swap removes a key referenced by `{DynamicResource}` | Re-evaluates to unset/default | Previous value stays painted | Accepted until the value-provider store (Tier 2.6) can revert to a lower-precedence value |
| `Style="{DynamicResource X}"` | Re-resolves style on change | Aliased to static; warns if missing at spawn | Accepted until dynamic style re-application exists |
| Viewbox | Scale participates in measure; child arranged at top-left; `StretchDirection` honored | Post-layout visual `UiTransform` scale about the center; `StretchDirection` ignored | Accepted approximation |
| Merged-dictionary isolation | Merged file resolves StaticResource against itself + app tier | Conformant (isolated stack) | Fixed per review |
| Sibling/diamond re-merges of one file | Legal, merged again | Conformant (memoized; only true cycles rejected) | Fixed per review |
| Local value vs style-setter `{DynamicResource}` on theme swap | Local wins forever | Conformant (style-tier entries dropped when a local value lands) | Fixed per review |

---

## 3. Parser/converter fixes to make now

Concrete, cheap, each with the distinguishing input (→ unit test). Files:
`crates/bevy_pf_xaml/src/markup.rs`, `value.rs`, `parser.rs`.

### Markup-extension scanner/parser (`markup.rs`)

| # | Fix | Distinguishing input | Expected (WPF) | WPF source |
|---|---|---|---|---|
| P1 | Accept `"` as an ME quote char alongside `'` (only the opening quote closes) | `Text='{Binding StringFormat="Hi, {0}"}'` | StringFormat = `Hi, {0}` | `MeScanner.cs:44-45,462-471` |
| P2 | Quoted value starting with `{` (not `{}`) → re-parse as nested ME | `{Binding RelativeSource='{RelativeSource Self}'}` | nested extension, not literal string | `MeScanner.cs:163-177,219`; `MePullParser.cs:350-359,405-414` |
| P3 | Strip leading `{}` from quoted tokens too | `{Binding StringFormat='{}{0}'}` | `{0}` | `MeScanner.cs:238,242-247` |
| P4 | `{` + whitespace IS an ME; bare `{` is a parse error (fix doc comment at `markup.rs:17-18` too) | `Text="{ Binding Title}"`; `Text="{"` | Binding; error | `XamlText.cs:164-178`; `MeScanner.cs:137-138,577-595` |
| P5 | Reject positional args after named args (remove test `accepts_positional_after_named_like_wpf`, `markup.rs:379-385`) | `{Binding Mode=OneWay, Name}` | error "unexpected token" | `MePullParser.cs:29-34,296-330,134` |
| P6 | Error on trailing characters (incl. whitespace) after the closing `}` | `Text="{Binding} "` | error | `MeScanner.cs:586-594`; `MePullParser.cs:59-62` |
| P7 | `,`/`=` inside balanced braces of an unquoted value → error, not content | `{Binding Path=a{1,2}}` | error | `MeScanner.cs:449-456,487-492` |
| P8 | Extension name terminates at `,`/`=`/`{` (then error), doesn't absorb them | `{Foo=Bar}` | error | `MeScanner.cs:423-456`; `MePullParser.cs:96-145` |
| P9 | Reject empty argument values | `{Binding Path=}`; `{Binding ,Path=x}` | error | `MeScanner.cs:179-200`; `MePullParser.cs:395-444` |
| P10 | Quote mid-bareword → error | `{Binding Path=a'b}` | error | `MeScanner.cs:462-467` |
| P11 | ME whitespace set = ASCII `{space, \t, \n, \r, \f}`, not Unicode | `{Binding\u{A0}Title}` | NBSP is part of the type name (unknown-type error) | `KnownStrings.cs:37`; `MeScanner.cs:516-520,561-575` |

### Length / GridLength / Thickness / CornerRadius (`value.rs`)

| # | Fix | Distinguishing input | Expected (WPF) | WPF source |
|---|---|---|---|---|
| P12 | Qualified units `px/in/cm/pt` (case-insensitive suffix; ×1, ×96, ×96/2.54, ×96/72) in lengths, Thickness components, GridLength | `Width="1in"` → 96; `Height="100px"` → 100 Pixel; `Margin="0.5cm,2,3,4"` | parses | `LengthConverter.cs:187-212`; `PixelUnit.cs`; `XamlGridLengthSerializer.cs:192-224,251` |
| P13 | `Auto` fully case-insensitive in `parse_f32` | `Margin="AUTO"` | NaN thickness | `LengthConverter.cs:195` |
| P14 | Consecutive/trailing list separators → error (replace filter-empties in `split_list`, `value.rs:11-15`) | `Margin="1,,2"`; `Margin="1,2,"` | error | `TokenizerHelper.cs:238-241,265-298` |
| P15 | GridLength rejects `NaN`/`Auto*` (plain float parse, no Auto/NaN aliases, `value.rs:454-462`) | `Height="NaN"`; `Height="Auto*"` | error | `XamlGridLengthSerializer.cs:240-242` + GridLength ctor |
| P16 | CornerRadius rejects `Auto` (raw float parse, `value.rs:112-131`) | `CornerRadius="Auto"` | error | `CornerRadiusConverter.cs:170` |
| P17 | `sc#` prefix is lowercase-only (`value.rs:181-188`) | `Color="SC#1,0,0,0"` | error (unknown color) | `Knowncolors.cs:240`; `Parsers.cs:141` |

### Whitespace handling (`parser.rs`)

| # | Fix | Distinguishing input | Expected (WPF) | WPF source |
|---|---|---|---|---|
| P20 | `xml:space="preserve"` inherits to descendants (thread parent flag into `convert_element`; child attr may override) | `<StackPanel xml:space="preserve"><TextBlock>  a  </TextBlock></StackPanel>` | spaces preserved | `XamlScanner.cs:214,765-773` |
| P21 | Whitespace-only text between inline elements survives as a single space in whitespace-significant collections | `<TextBlock><Bold>a</Bold> <Bold>b</Bold></TextBlock>` | "a b", not "ab" | `XamlPullParser.cs:1123-1210`; `XamlText.cs:107-113` |
| P23 | Strip `\r` in the preserve path (one-liner; only observable via `&#13;`) | `xml:space="preserve"` text with `&#13;` | `\r` removed | `XamlText.cs:81-105` |

---

## 4. Template-system decisions settled by WPF source

These confirm or **correct** `docs/roadmap-from-noesis-analysis.md` (Tiers 2.6/2.7/3). Roadmap
deltas are called out explicitly.

### 4.1 Value precedence: adopt `BaseValueSourceInternal` verbatim

`WindowsBase/System/Windows/EffectiveValueEntry.cs:613-641`:

```
Default=1 < Inherited=2 < ThemeStyle=3 < ThemeStyleTrigger=4 < Style=5
< TemplateTrigger=6 < StyleTrigger=7 < ImplicitReference=8
< ParentTemplate=9 < ParentTemplateTrigger=10 < Local=11
```

**Corrections to roadmap 2.6's draft (`local > animation > template-trigger > style-trigger > style-setter > inherited > default`):**

1. **StyleTrigger (7) outranks TemplateTrigger (6)** — the roadmap had it backwards. Confirmed by lookup order in `StyleHelper.cs:3846-3935`.
2. **Animation is not a base source.** It's a modifier bit (`FullValueSource.IsAnimated = 0x20`, `EffectiveValueEntry.cs:591-607`) layered above *any* base value, with the pre-animation value retained in `ModifiedValue.BaseValue` for revert/handoff. Model the store as: base-source slot + Expression/Animated/Coerced modifier stack. (This is what makes Storyboard `HandoffBehavior`/revert-on-stop fall out for free — roadmap Tier 3's intuition was right, the slot layout wasn't.)
3. `ParentTemplate(9)`/`ParentTemplateTrigger(10)` apply only to **template children**: TemplateBinding value = 9; template trigger with TargetName = 10 — both just under Local.

### 4.2 Trigger evaluation order: "walk backwards, first active wins"

One lookup list per (childIndex, property): BasedOn-chain setters inserted first, then triggers
in declaration order, base style before derived (`Style.cs:680-744`, both passes recurse into
`_basedOn` first). Resolution scans the list **backwards**, returning the first setter or active
trigger (`StyleHelper.cs:2618`). Net rules:

- last-declared active trigger wins ties;
- any active trigger beats any setter;
- derived style beats base;
- MultiTrigger = AND of conditions (`StyleHelper.cs:2645-2650`);
- Trigger Enter/ExitActions are a **separate edge-triggered list** (`Style.cs:757+`), not part of value lookup.

### 4.3 Pull model with shared sealed tables, not per-entity stamping

`FrameworkElement.GetRawValue` (`FrameworkElement.cs:1812-1915`) computes effective values on
demand: local → templated-parent shared table → own style/template-trigger/theme-style →
implicit style reference (Style property only) → inherited → default. Template children carry
only `(TemplatedParent, TemplateChildIndex)`; their style-driven values come from the template's
shared `ChildRecordFromChildIndex` table built once at seal. TemplateBinding is just a
`ValueLookupType.TemplateBinding` row in that table (`StyleHelper.cs:5687-5696`), **not** a
per-child binding object. For bevy_pf: one immutable table per sealed style/template + a small
per-entity cache; invalidation via precomputed dependent lists.

### 4.4 Template namescope: per-expansion, isolated, index-addressed

Confirms roadmap 2.7's "per-expansion, isolated" decision, and adds: **resolve names to child
indices at seal time, not strings at runtime.** At seal each named template element gets a
stable ChildIndex (container=0, root=1, then registration order — `FrameworkElement.cs:573`,
`TemplateNameScope.cs:151-161`). At expansion a fresh `TemplateNameScope` per templated parent
(`FrameworkTemplate.cs:905`); names never merge into the page namescope — lookup only via
`template.FindName(name, templatedParent)` / `GetTemplateChild`. Trigger `TargetName` **and**
`SourceName` both resolve through `ChildIndexFromChildName` (note: `SourceName` means trigger
*conditions* can come from named template children, e.g. `Trigger SourceName="PART_Popup"` in
Fluent ComboBox).

### 4.5 Seal aggressively; validate at seal; tolerate missing parts at apply

- `Style.Seal()` (`Style.cs:478-553`): require TargetType; BasedOn.TargetType assignable from TargetType; explicit BasedOn cycle check; seal everything; build tables in fixed order.
- Style triggers may **not** use TargetName/SourceName — hard error (`Style.cs:733-744`).
- `ControlTemplate.ValidateTemplatedParent` throws on type mismatch (`ControlTemplate.cs:58-75`).
- But at apply time the opposite posture: **every `OnApplyTemplate` null-tolerates absent PART_s** — a missing part degrades, never panics. Mirror both postures.

### 4.6 PART_ contract table (authoritative, from `[TemplatePart]` + `GetTemplateChild` call sites)

| Control | Part | Type | Purpose |
|---|---|---|---|
| ButtonBase family (Button, RepeatButton, ToggleButton, CheckBox, RadioButton) | *(none)* | — | all behavior via control class + triggers |
| TextBox / PasswordBox | `PART_ContentHost` | FrameworkElement (ScrollViewer/Decorator) | host for injected text-editing view (`TextBoxBase.cs:33,1886`) |
| ProgressBar | `PART_Track`, `PART_Indicator`, `PART_GlowRect` | FrameworkElement | code sets Indicator width = fraction of Track (`ProgressBar.cs:26-28,340-342`) |
| Slider | `PART_Track` (Track), `PART_SelectionRange` (+ code also probes `PART_SelectedRange`) | — | Track does thumb positioning + hit-test-to-value (`Slider.cs:30-31,1328-1330`) |
| ScrollBar | `PART_Track` (Track) | — | only code contract; line/page buttons wired by **routed commands**, not names (`ScrollBar.cs:26,740`) |
| ScrollViewer | `PART_HorizontalScrollBar`, `PART_VerticalScrollBar` (ScrollBar), `PART_ScrollContentPresenter` | — | scrollbars driven by TemplateBinding + `{Binding ... RelativeSource TemplatedParent}` (`ScrollViewer.cs:59-61,935,1366-1370`) |
| ComboBox | `PART_EditableTextBox` (TextBox), `PART_Popup` (Popup) | — | dropdown = Popup `IsOpen="{TemplateBinding IsDropDownOpen}"`; toggle is a *non-contract* ToggleButton with a **two-way** TemplatedParent binding (`ComboBox.cs:26-27,1612-1619`) |
| Expander | *(no attribute)*; code looks up ToggleButton `HeaderSite` | — | `IsChecked="{Binding IsExpanded, Mode=TwoWay, RelativeSource TemplatedParent}"` (`Expander.cs:310,349`) |
| ListBox/ListBoxItem, GroupBox, Label | *(none)* | — | headers via `ContentPresenter ContentSource="Header"`; selection via `IsSelected` triggers |

Cross-cutting: `ContentPresenter` with no explicit binding auto-binds the templated parent's
`Content`; `ContentSource="Header"` remaps to `Header`/`HeaderTemplate`. **Two-way
`RelativeSource TemplatedParent` bindings are the standard write-back path** — the template
engine must support that direction, not only TemplateBinding.

### 4.7 Fluent-theme requirements ranked (what to build, in order)

1. ControlTemplate expansion + TemplateBinding + `OverridesDefaultStyle` (every Fluent style needs it), incl. qualified/attached setter properties (`<Setter Property="Border.CornerRadius">` read back via `{TemplateBinding Border.CornerRadius}` — properties the target type doesn't own). Extends roadmap Tier 1 item 7's qualified-setter work.
2. Value-provider store + `ControlTemplate.Triggers`: `TargetName`, MultiTrigger, triggers on `Content="{x:Null}"`/`Content=""`, and `Trigger SourceName=` (conditions from named template children — roadmap 2.6 didn't include SourceName).
3. **DynamicResource as deferred, re-resolvable lookup** — 653 refs in `Fluent.Light.xaml`; mandatory even for a single static theme because ThemeManager merges color dictionaries *after* loading styles (`ThemeManager.cs:182-190`). We currently alias DynamicResource to StaticResource (`crates/bevy_pf/src/resources.rs:228`) — that alias cannot survive theme work.
4. Popup + Track/Thumb/RepeatButton primitives + routed commands (`ScrollBar.PageUpCommand` etc.). Track is load-bearing: ScrollBar/Slider templates are thin, the logic lives in Track layout.
5. Storyboards in `Trigger.EnterActions/ExitActions` with sub-property paths (`(Border.Background).(SolidColorBrush.Opacity)`); Expander's content reveal itself is storyboard-driven.
6. VisualStateManager (small surface for our control set: ProgressBar Indeterminate, Slider thumb, RadioButton).
7. Key-as-object resource keys (`x:Key="{x:Static SystemParameters.FocusVisualStyleKey}"`, ComponentResourceKey).
8. FocusVisualStyle + keyboard focus adorner layer.
9. Icon font story (Segoe Fluent Icons private-use glyphs via `SymbolThemeFontFamily`).
10. MultiBinding + theme converters (warn-and-degrade acceptable except Expander animation).
11. ThemeMode plumbing (`{None, Light, Dark, System}`) — trivial once URI work (roadmap 2.1) lands.

### 4.8 Acceptance-target dictionaries

Only five complete standalone theme dictionaries exist in dotnet/wpf; everything else is
generator-stitched fragments.

1. `Themes/PresentationFramework.Fluent/Themes/Fluent.Light.xaml` (5,972 lines) — **primary gate**: fully flattened, zero merges, pure Border/Grid/Path templates, no native chrome.
2. `Fluent.Dark.xaml` — theme-swap test (every DynamicResource re-resolves).
3. `Fluent.HC.xaml` — cheap third data point.
4. `Fluent.xaml` (System) — deliberately incomplete alone; excellent MergedDictionaries + DynamicResource combined test.
5. `Aero.NormalColor.xaml` — parser-robustness corpus **only** (depends on code-backed chrome elements).

Staged gates: (1) each `Fluent/Styles/*.xaml` fragment parses+instantiates merged after
`Resources/Variables.xaml` + `Resources/Theme/Light.xaml`; (2) `Fluent.Light.xaml` loads with
zero template/trigger/TemplateBinding warnings; (3) Button/CheckBox/TextBox/ListBoxItem render
with hover/press/disable state changes via the provider store; (4) Light↔Dark swap re-resolves
DynamicResources; (5) ScrollBar/ComboBox/Expander once Popup/Track/Storyboard land.

---

## 5. Corpus

Harvested verbatim (byte-identical) from `dotnet/wpf` commit
`07ae487dddc7a805ad1c98ed31cd09b298cb4c33` (MIT) into `tests/corpus/wpf-upstream/`, with
attribution in `tests/corpus/wpf-upstream/README.md`. Guarded by
`crates/bevy_pf_xaml/tests/corpus.rs::parses_all_wpf_upstream_corpus_files` (parse-only for
ResourceDictionary-rooted files; deliberately excluded from the instantiation test, which reads
`tests/corpus/wpf/` non-recursively).

| Corpus file | Guards |
|---|---|
| `template_app.xaml` | Application root, StartupUri, empty Resources property element |
| `template_mainwindow.xaml` | Window + designer namespaces (`mc:Ignorable="d"`) |
| `template_customcontrol_generic.xaml` | minimal theme RD: implicit `{x:Type}` style, ControlTemplate, TemplateBinding |
| `sample_resourcedictionary.xaml` | Style/BasedOn, Trigger, MultiTrigger, DataTrigger, ControlTemplate.Triggers w/ TargetName, DataTemplate |
| `drt_bracket_character_attribute.xaml` | ME boundary stress: custom bracket chars, quoted/nested brackets, `,`/`=` inside `[...]`/`(...)` |
| `presentationui_classic_theme.xaml` | ComponentResourceKey as x:Key, `{StaticResource {x:Static ...}}`, bare `xmlns:sys="System"` |
| `presentationui_findtoolbar.xaml` | shipped UI: ToolBar root, x:ClassModifier/x:Uid/xml:lang, attached props on root |
| `ribbon_aero_theme.xaml` | complete ~50 KB theme RD: 16 ControlTemplates, 42 trigger constructs, entity-escaped `x:Key`, MultiDataTrigger |
| `fluent_button_styles.xaml` | .NET 9 Fluent standalone RD, DynamicResource-heavy |
| `fluent_expander_styles.xaml` | Storyboard stress: 96 refs, Enter/ExitActions, paths like `(FrameworkElement.LayoutTransform).(RotateTransform.Angle)` |

Result at harvest time: all 10 parse clean; an extended sweep over all of `src/Themes`,
`src/PresentationUI`, and Ribbon themes (171 files incl. 300-900 KB generated dictionaries)
produced zero hard parser failures. **Caution:** because the ME parser is currently *laxer* than
WPF in several ways (§3 P4-P10), "parses clean" over-accepts; the P-fixes above add the missing
strictness and their unit tests are the real conformance guard.
