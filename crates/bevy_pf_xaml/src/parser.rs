//! XML parsing with XAML semantics applied.

use roxmltree::{Document, Node, NodeType, ParsingOptions};

use crate::ast::*;
use crate::error::{TextPos, XamlError, XamlResult};
use crate::markup;

/// Parse XAML source text into a [`XamlDocument`].
pub fn parse(source: &str) -> XamlResult<XamlDocument> {
    // Files saved by Visual Studio commonly start with a UTF-8 BOM.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let opts = ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    let doc = Document::parse_with_options(source, opts)?;
    let root = doc.root_element();
    let ctx = Ctx { doc: &doc };
    let root = ctx.convert_element(root, false)?;
    Ok(XamlDocument { root })
}

struct Ctx<'a, 'input> {
    doc: &'a Document<'input>,
}

impl<'a, 'input> Ctx<'a, 'input> {
    fn pos_of(&self, node: Node) -> TextPos {
        let p = self.doc.text_pos_at(node.range().start);
        TextPos::new(p.row, p.col)
    }

    fn resolve_ns(&self, uri: Option<&str>) -> NsKind {
        match uri {
            None | Some("") | Some(NS_PRESENTATION) | Some(NS_PRESENTATION_ALT) => {
                NsKind::Keep(XamlNamespace::Default)
            }
            Some(NS_XAML) | Some(NS_XAML_WINUI) => NsKind::Keep(XamlNamespace::Xaml),
            Some(NS_MC) | Some(NS_DESIGN) => NsKind::Ignore,
            Some(other) => NsKind::Keep(XamlNamespace::Custom(other.to_string())),
        }
    }

    fn convert_element(&self, node: Node, inherited_preserve: bool) -> XamlResult<XamlNode> {
        let pos = self.pos_of(node);
        let tag = node.tag_name();
        let namespace = match self.resolve_ns(tag.namespace()) {
            NsKind::Keep(ns) => ns,
            NsKind::Ignore => {
                return Err(XamlError::parse(
                    pos,
                    format!(
                        "element `{}` is in an ignorable namespace and cannot be instantiated",
                        tag.name()
                    ),
                ));
            }
        };

        let local = tag.name();
        if let Some((owner, _prop)) = local.split_once('.') {
            return Err(XamlError::parse(
                pos,
                format!(
                    "property element `{local}` is not valid here; it must be a direct child of a `{owner}` element"
                ),
            ));
        }

        let mut out = XamlNode {
            name: local.to_string(),
            namespace,
            x_name: None,
            x_key: None,
            x_uid: None,
            x_class: None,
            attributes: Vec::new(),
            property_elements: Vec::new(),
            children: Vec::new(),
            pos,
        };

        // xml:space inherits to descendants; a child's own attribute
        // overrides in either direction (XamlScanner.cs:765-773).
        let preserve_space = node
            .attributes()
            .find(|a| {
                a.namespace() == Some("http://www.w3.org/XML/1998/namespace") && a.name() == "space"
            })
            .map(|a| a.value() == "preserve")
            .unwrap_or(inherited_preserve);

        for attr in node.attributes() {
            let apos = {
                let p = self.doc.text_pos_at(attr.range().start);
                TextPos::new(p.row, p.col)
            };
            let ns = match self.resolve_ns(attr.namespace()) {
                NsKind::Keep(ns) => ns,
                NsKind::Ignore => continue, // mc:Ignorable, d:DesignWidth, ...
            };
            // xml:space / xml:lang
            if attr.namespace() == Some("http://www.w3.org/XML/1998/namespace") {
                continue;
            }
            let name = attr.name();
            if ns == XamlNamespace::Xaml {
                match name {
                    "Name" => {
                        out.x_name = Some(attr.value().to_string());
                        continue;
                    }
                    "Key" => {
                        out.x_key = Some(attr.value().to_string());
                        continue;
                    }
                    "Class" => {
                        out.x_class = Some(attr.value().to_string());
                        continue;
                    }
                    "Uid" => {
                        out.x_uid = Some(attr.value().to_string());
                        continue;
                    }
                    // x:FieldModifier, x:Shared, x:TypeArguments, ...
                    _ => {}
                }
            } else if ns == XamlNamespace::Default && name == "Name" {
                // FrameworkElement.Name is an alias for x:Name.
                out.x_name = Some(attr.value().to_string());
                continue;
            }

            let (owner, prop_name) = match name.split_once('.') {
                Some((owner, prop)) => (Some(owner.to_string()), prop.to_string()),
                None => (None, name.to_string()),
            };

            let raw = attr.value();
            let value = if markup::is_extension(raw) {
                XamlValue::Extension(markup::parse_extension(raw)?)
            } else {
                XamlValue::Str(markup::unescape_literal(raw).to_string())
            };

            out.attributes.push(XamlAttribute {
                owner,
                name: prop_name,
                namespace: ns,
                value,
                pos: apos,
            });
        }

        self.convert_children(node, preserve_space, &mut out)?;
        Ok(out)
    }

    fn convert_children(
        &self,
        node: Node,
        preserve_space: bool,
        out: &mut XamlNode,
    ) -> XamlResult<()> {
        let mut children: Vec<XamlChild> = Vec::new();

        for child in node.children() {
            match child.node_type() {
                NodeType::Comment | NodeType::PI => {}
                NodeType::Text => {
                    let raw = child.text().unwrap_or("");
                    if preserve_space {
                        // `\r` is stripped even in the preserve path
                        // (XamlText.cs:81-105).
                        children.push(XamlChild::Text(raw.replace('\r', "")));
                    } else {
                        let collapsed = collapse_whitespace(raw);
                        if !collapsed.is_empty() {
                            children.push(XamlChild::Text(collapsed));
                        } else if !raw.is_empty() {
                            // Whitespace-only run: kept as a single space so
                            // that space between inline elements survives
                            // (`<Bold>a</Bold> <Bold>b</Bold>` -> "a b");
                            // normalize_whitespace_children drops it unless
                            // it sits directly between two elements.
                            children.push(XamlChild::Text(" ".to_string()));
                        }
                    }
                }
                NodeType::Element => {
                    let tag = child.tag_name();
                    if let NsKind::Ignore = self.resolve_ns(tag.namespace()) {
                        continue;
                    }
                    let local = tag.name();
                    if let Some((owner, prop)) = local.split_once('.') {
                        // Property-element syntax.
                        let ppos = self.pos_of(child);
                        if child.attributes().next().is_some() {
                            return Err(XamlError::parse(
                                ppos,
                                format!("property element `{local}` cannot have attributes"),
                            ));
                        }
                        let mut prop_el = XamlPropertyElement {
                            owner: owner.to_string(),
                            name: prop.to_string(),
                            values: Vec::new(),
                            pos: ppos,
                        };
                        let mut tmp = XamlNode {
                            name: String::new(),
                            namespace: XamlNamespace::Default,
                            x_name: None,
                            x_key: None,
                            x_uid: None,
                            x_class: None,
                            attributes: Vec::new(),
                            property_elements: Vec::new(),
                            children: Vec::new(),
                            pos: ppos,
                        };
                        self.convert_children(child, preserve_space, &mut tmp)?;
                        if !tmp.property_elements.is_empty() {
                            return Err(XamlError::parse(
                                ppos,
                                format!(
                                    "property element `{local}` cannot directly contain another property element"
                                ),
                            ));
                        }
                        prop_el.values = tmp.children;
                        out.property_elements.push(prop_el);
                    } else {
                        children.push(XamlChild::Element(
                            self.convert_element(child, preserve_space)?,
                        ));
                    }
                }
                NodeType::Root => {}
            }
        }

        if !preserve_space {
            normalize_whitespace_children(&mut children);
        }
        out.children = children;
        Ok(())
    }
}

enum NsKind {
    Keep(XamlNamespace),
    Ignore,
}

/// Collapse runs of XML whitespace into single spaces (XAML text handling).
/// Returns an empty string if the input is whitespace-only.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    let mut has_content = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            in_ws = true;
        } else {
            if in_ws && has_content {
                out.push(' ');
            } else if in_ws && !has_content {
                // Leading whitespace within this run: keep a single marker
                // space; edge trimming decides whether it survives.
                out.push(' ');
            }
            in_ws = false;
            has_content = true;
            out.push(c);
        }
    }
    if !has_content {
        return String::new();
    }
    if in_ws {
        out.push(' ');
    }
    out
}

/// WPF edge-trim and whitespace-run rules: trim the leading whitespace of the
/// first text child and the trailing whitespace of the last; keep a
/// whitespace-only run (as one space) only when it sits directly between two
/// element children (`XamlPullParser.cs:1123-1210`). Whether that space is
/// *used* is up to the consumer (inline collections keep it; panels skip it).
fn normalize_whitespace_children(children: &mut Vec<XamlChild>) {
    if let Some(XamlChild::Text(t)) = children.first_mut() {
        let trimmed = t.trim_start().to_string();
        *t = trimmed;
    }
    if let Some(XamlChild::Text(t)) = children.last_mut() {
        let trimmed = t.trim_end().to_string();
        *t = trimmed;
    }
    let keep: Vec<bool> = (0..children.len())
        .map(|i| match &children[i] {
            XamlChild::Text(t) if t.trim().is_empty() => {
                let prev_is_element = i > 0 && matches!(children[i - 1], XamlChild::Element(_));
                let next_is_element =
                    i + 1 < children.len() && matches!(children[i + 1], XamlChild::Element(_));
                !t.is_empty() && prev_is_element && next_is_element
            }
            XamlChild::Text(t) => !t.is_empty(),
            XamlChild::Element(_) => true,
        })
        .collect();
    let mut index = 0;
    children.retain(|_| {
        let k = keep[index];
        index += 1;
        k
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::{MarkupExtension, MarkupValue};

    #[test]
    fn parses_minimal_window() {
        let doc = parse(
            r#"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        x:Class="MyApp.MainWindow"
                        Title="Hello" Width="800" Height="450">
                 <Grid/>
               </Window>"#,
        )
        .unwrap();
        let root = &doc.root;
        assert_eq!(root.name, "Window");
        assert_eq!(root.namespace, XamlNamespace::Default);
        assert_eq!(root.x_class.as_deref(), Some("MyApp.MainWindow"));
        assert_eq!(
            root.attribute("Title"),
            Some(&XamlValue::Str("Hello".into()))
        );
        assert_eq!(root.child_elements().count(), 1);
        assert_eq!(root.child_elements().next().unwrap().name, "Grid");
    }

    #[test]
    fn parses_text_content() {
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 Hello,
                 World!
               </TextBlock>"#,
        )
        .unwrap();
        assert_eq!(doc.root.text_content().as_deref(), Some("Hello, World!"));
    }

    #[test]
    fn mixed_inline_content_preserves_interior_spaces() {
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">Hello <Bold>World</Bold>!</TextBlock>"#,
        )
        .unwrap();
        let kids = &doc.root.children;
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].as_text(), Some("Hello "));
        assert_eq!(kids[1].as_element().unwrap().name, "Bold");
        assert_eq!(kids[2].as_text(), Some("!"));
    }

    #[test]
    fn parses_attached_properties() {
        let doc = parse(
            r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <Button Grid.Row="1" Grid.Column="2" Grid.ColumnSpan="2" Content="Go"/>
               </Grid>"#,
        )
        .unwrap();
        let button = doc.root.child_elements().next().unwrap();
        assert_eq!(
            button.attached_attribute("Grid", "Row"),
            Some(&XamlValue::Str("1".into()))
        );
        assert_eq!(
            button.attached_attribute("Grid", "ColumnSpan"),
            Some(&XamlValue::Str("2".into()))
        );
        assert_eq!(
            button.attribute("Content"),
            Some(&XamlValue::Str("Go".into()))
        );
    }

    #[test]
    fn parses_property_element_syntax() {
        let doc = parse(
            r#"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <Button.Content>
                   <StackPanel><TextBlock Text="Hi"/></StackPanel>
                 </Button.Content>
               </Button>"#,
        )
        .unwrap();
        let pe = doc.root.property_element("Content").unwrap();
        assert_eq!(pe.owner, "Button");
        let sp = pe.single_element().unwrap();
        assert_eq!(sp.name, "StackPanel");
    }

    #[test]
    fn parses_attached_property_elements() {
        let doc = parse(
            r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <Grid.RowDefinitions>
                   <RowDefinition Height="Auto"/>
                   <RowDefinition Height="*"/>
                 </Grid.RowDefinitions>
               </Grid>"#,
        )
        .unwrap();
        let rows = doc.root.property_element("RowDefinitions").unwrap();
        assert_eq!(rows.elements().count(), 2);
        assert_eq!(
            rows.elements().next().unwrap().attribute("Height"),
            Some(&XamlValue::Str("Auto".into()))
        );
    }

    #[test]
    fn parses_markup_extension_attributes() {
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          Text="{Binding Title}"
                          Background="{StaticResource BgBrush}"/>"#,
        )
        .unwrap();
        let text = doc.root.attribute("Text").unwrap();
        let XamlValue::Extension(ext) = text else {
            panic!("expected extension")
        };
        assert_eq!(ext.name, "Binding");
        assert_eq!(ext.positional, vec![MarkupValue::Str("Title".to_string())]);
    }

    #[test]
    fn escape_prefix_is_literal() {
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          Text="{}{not a binding}"/>"#,
        )
        .unwrap();
        assert_eq!(
            doc.root.attribute("Text"),
            Some(&XamlValue::Str("{not a binding}".into()))
        );
    }

    #[test]
    fn skips_design_time_attributes() {
        let doc = parse(
            r#"<UserControl
                 xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                 xmlns:d="http://schemas.microsoft.com/expression/blend/2008"
                 xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
                 mc:Ignorable="d"
                 d:DesignHeight="450" d:DesignWidth="800">
               </UserControl>"#,
        )
        .unwrap();
        assert!(doc.root.attributes.is_empty());
    }

    #[test]
    fn name_and_x_name_are_aliases() {
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
                 <Button Name="a"/>
                 <Button x:Name="b"/>
               </StackPanel>"#,
        )
        .unwrap();
        let names: Vec<_> = doc
            .root
            .child_elements()
            .map(|e| e.x_name.clone().unwrap())
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn parses_resource_keys() {
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
                 <StackPanel.Resources>
                   <SolidColorBrush x:Key="BgBrush" Color="AliceBlue"/>
                 </StackPanel.Resources>
               </StackPanel>"#,
        )
        .unwrap();
        let res = doc.root.property_element("Resources").unwrap();
        let brush = res.single_element().unwrap();
        assert_eq!(brush.name, "SolidColorBrush");
        assert_eq!(brush.x_key.as_deref(), Some("BgBrush"));
    }

    #[test]
    fn xaml_namespace_primitive_elements() {
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
                 <StackPanel.Resources>
                   <x:Double x:Key="Size">42.5</x:Double>
                   <x:String x:Key="Greeting">Hi</x:String>
                 </StackPanel.Resources>
               </StackPanel>"#,
        )
        .unwrap();
        let res = doc.root.property_element("Resources").unwrap();
        let items: Vec<_> = res.elements().collect();
        assert_eq!(items[0].namespace, XamlNamespace::Xaml);
        assert_eq!(items[0].name, "Double");
        assert_eq!(items[0].text_content().as_deref(), Some("42.5"));
        assert_eq!(items[1].text_content().as_deref(), Some("Hi"));
    }

    #[test]
    fn alternate_presentation_namespace_accepted() {
        let doc = parse(
            r#"<UserControl xmlns="https://github.com/avaloniaui">
                 <Button Content="Hello"/>
               </UserControl>"#,
        )
        .unwrap();
        assert_eq!(doc.root.namespace, XamlNamespace::Default);
    }

    #[test]
    fn custom_clr_namespace() {
        let doc = parse(
            r#"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:local="clr-namespace:MyApp.Controls">
                 <local:FancyControl/>
               </Window>"#,
        )
        .unwrap();
        let child = doc.root.child_elements().next().unwrap();
        assert_eq!(child.name, "FancyControl");
        assert_eq!(
            child.namespace,
            XamlNamespace::Custom("clr-namespace:MyApp.Controls".to_string())
        );
    }

    #[test]
    fn whitespace_between_inline_elements_survives() {
        // XamlPullParser.cs:1123-1210 — `<Bold>a</Bold> <Bold>b</Bold>`
        // keeps a single space between the inlines.
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><Bold>a</Bold> <Bold>b</Bold></TextBlock>"#,
        )
        .unwrap();
        let kids = &doc.root.children;
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[1].as_text(), Some(" "));
    }

    #[test]
    fn whitespace_between_panel_children_collapses_to_marker_space() {
        // Between two elements the run is kept (consumers decide relevance);
        // leading/trailing runs are dropped.
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <Button/>
                 <Button/>
               </StackPanel>"#,
        )
        .unwrap();
        assert_eq!(doc.root.children.len(), 3);
        assert_eq!(doc.root.children[1].as_text(), Some(" "));
        assert_eq!(doc.root.child_elements().count(), 2);
    }

    #[test]
    fn xml_space_preserve_inherits_to_descendants() {
        // XamlScanner.cs:765-773 — xml:space flows down; children can
        // override it back to default.
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xml:space="preserve"><TextBlock>  a  b  </TextBlock><TextBlock xml:space="default">  a  b  </TextBlock></StackPanel>"#,
        )
        .unwrap();
        let kids: Vec<_> = doc.root.child_elements().collect();
        assert_eq!(kids[0].text_content().as_deref(), Some("  a  b  "));
        assert_eq!(kids[1].text_content().as_deref(), Some("a b"));
    }

    #[test]
    fn preserve_strips_carriage_returns() {
        // XamlText.cs:81-105 — `\r` (here via &#13;) is stripped even when
        // whitespace is preserved.
        let doc = parse(
            "<TextBlock xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" xml:space=\"preserve\">a&#13;\nb</TextBlock>",
        )
        .unwrap();
        assert_eq!(doc.root.text_content().as_deref(), Some("a\nb"));
    }

    #[test]
    fn comments_are_skipped() {
        let doc = parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <!-- a comment -->
                 <Button/>
               </StackPanel>"#,
        )
        .unwrap();
        assert_eq!(doc.root.children.len(), 1);
    }

    #[test]
    fn rejects_property_element_as_root_child_of_wrong_parent() {
        // A dotted element used as ordinary content is still parsed as a
        // property element of its parent; a dotted ROOT element is an error.
        assert!(parse(
            r#"<Button.Content xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#
        )
        .is_err());
    }

    #[test]
    fn rejects_attributes_on_property_elements() {
        assert!(
            parse(
                r#"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <Button.Content Foo="bar"><TextBlock/></Button.Content>
               </Button>"#
            )
            .is_err()
        );
    }

    #[test]
    fn error_positions_are_reported() {
        let err = parse("<Button xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" Text=\"{Binding\"/>").unwrap_err();
        match err {
            XamlError::MarkupExtension { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn nested_extension_roundtrip_through_parser() {
        let doc = parse(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          Text="{Binding Path=Name, Converter={StaticResource conv}, ConverterParameter='a,b'}"/>"#,
        )
        .unwrap();
        let XamlValue::Extension(ext) = doc.root.attribute("Text").unwrap() else {
            panic!()
        };
        assert_eq!(
            ext.arg("ConverterParameter"),
            Some(&MarkupValue::Str("a,b".to_string()))
        );
        assert!(matches!(
            ext.arg("Converter"),
            Some(MarkupValue::Extension(e)) if e.name == "StaticResource"
        ));
        let _: &MarkupExtension = ext;
    }
}
