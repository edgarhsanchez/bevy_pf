//! The XAML abstract syntax tree.
//!
//! The parser resolves XML namespaces into [`XamlNamespace`] categories,
//! splits property-element syntax (`<Button.Content>`) and attached
//! properties (`Grid.Row="1"`) into structured forms, and parses markup
//! extensions in attribute values.

use crate::error::TextPos;
use crate::markup::MarkupExtension;

/// The WPF presentation namespace (also used by WinUI/UWP XAML).
pub const NS_PRESENTATION: &str = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
/// The XAML language namespace, conventionally mapped to `x:`.
pub const NS_XAML: &str = "http://schemas.microsoft.com/winfx/2006/xaml";
/// WinUI 3 XAML language namespace variant.
pub const NS_XAML_WINUI: &str = "http://schemas.microsoft.com/winfx/2009/xaml";
/// Markup-compatibility namespace (`mc:`); `mc:Ignorable` content is skipped.
pub const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// Blend design-time namespace (`d:`); ignored.
pub const NS_DESIGN: &str = "http://schemas.microsoft.com/expression/blend/2008";
/// An alternate default namespace, accepted as an alias of the presentation
/// namespace so documents written against either one instantiate.
pub const NS_PRESENTATION_ALT: &str = "https://github.com/avaloniaui";

/// Which logical namespace an element or attribute belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XamlNamespace {
    /// The default presentation namespace (WPF / WinUI UI types).
    Default,
    /// The XAML language namespace (`x:` — `x:Name`, `x:Key`, `x:Double`, ...).
    Xaml,
    /// A custom namespace: `clr-namespace:...` or any other URI.
    Custom(String),
}

/// A parsed XAML document.
#[derive(Debug, Clone, PartialEq)]
pub struct XamlDocument {
    pub root: XamlNode,
}

/// Either a child element or a run of character data.
#[derive(Debug, Clone, PartialEq)]
pub enum XamlChild {
    Element(XamlNode),
    /// Whitespace-normalized text content (e.g. `<TextBlock>Hello</TextBlock>`,
    /// or the text runs interleaved with inline elements).
    Text(String),
}

impl XamlChild {
    pub fn as_element(&self) -> Option<&XamlNode> {
        match self {
            XamlChild::Element(n) => Some(n),
            XamlChild::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            XamlChild::Text(t) => Some(t),
            XamlChild::Element(_) => None,
        }
    }
}

/// An element in the XAML tree, e.g. `<Button Content="Hi"/>`.
#[derive(Debug, Clone, PartialEq)]
pub struct XamlNode {
    /// Local type name, e.g. `Button`.
    pub name: String,
    /// Resolved namespace of the element.
    pub namespace: XamlNamespace,
    /// `x:Name` (or the WPF `Name` attribute, which is an alias).
    pub x_name: Option<String>,
    /// `x:Key` (for resource-dictionary entries).
    pub x_key: Option<String>,
    /// `x:Uid` (localization / stable identification).
    pub x_uid: Option<String>,
    /// `x:Class` (on root elements).
    pub x_class: Option<String>,
    /// Attribute-syntax properties, including attached ones (`Grid.Row="1"`).
    pub attributes: Vec<XamlAttribute>,
    /// Property-element-syntax properties (`<Button.Content>...</Button.Content>`).
    pub property_elements: Vec<XamlPropertyElement>,
    /// Content children (elements and text) in document order.
    pub children: Vec<XamlChild>,
    /// Source position of the opening tag.
    pub pos: TextPos,
}

impl XamlNode {
    /// Look up an attribute-syntax property by local name (non-attached).
    pub fn attribute(&self, name: &str) -> Option<&XamlValue> {
        self.attributes
            .iter()
            .find(|a| a.owner.is_none() && a.name == name)
            .map(|a| &a.value)
    }

    /// Look up an attached attribute like `Grid.Row`.
    pub fn attached_attribute(&self, owner: &str, name: &str) -> Option<&XamlValue> {
        self.attributes
            .iter()
            .find(|a| a.owner.as_deref() == Some(owner) && a.name == name)
            .map(|a| &a.value)
    }

    /// Look up a property element by name; `owner` must match either this
    /// element's type (regular property) or the attached owner type.
    pub fn property_element(&self, name: &str) -> Option<&XamlPropertyElement> {
        self.property_elements.iter().find(|p| p.name == name)
    }

    /// Content children that are elements.
    pub fn child_elements(&self) -> impl Iterator<Item = &XamlNode> {
        self.children.iter().filter_map(XamlChild::as_element)
    }

    /// The merged text content, if this element holds only text.
    pub fn text_content(&self) -> Option<String> {
        let mut out = String::new();
        for c in &self.children {
            match c {
                XamlChild::Text(t) => out.push_str(t),
                XamlChild::Element(_) => return None,
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }
}

/// An attribute-syntax property.
#[derive(Debug, Clone, PartialEq)]
pub struct XamlAttribute {
    /// `Some("Grid")` for attached syntax `Grid.Row="1"`; `None` for plain
    /// attributes.
    pub owner: Option<String>,
    /// Property name, e.g. `Content` or `Row`.
    pub name: String,
    /// Namespace the *attribute* was written in (e.g. `x:Uid` -> Xaml).
    pub namespace: XamlNamespace,
    pub value: XamlValue,
    pub pos: TextPos,
}

/// An attribute value: either a literal string or a parsed markup extension.
#[derive(Debug, Clone, PartialEq)]
pub enum XamlValue {
    Str(String),
    Extension(MarkupExtension),
}

impl XamlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            XamlValue::Str(s) => Some(s),
            XamlValue::Extension(_) => None,
        }
    }

    pub fn as_extension(&self) -> Option<&MarkupExtension> {
        match self {
            XamlValue::Extension(e) => Some(e),
            XamlValue::Str(_) => None,
        }
    }
}

/// A property set with property-element syntax:
/// `<Button.Content>...</Button.Content>` or `<Grid.RowDefinitions>...</Grid.RowDefinitions>`.
#[derive(Debug, Clone, PartialEq)]
pub struct XamlPropertyElement {
    /// The type before the dot (`Button`, `Grid`). For attached properties this
    /// differs from the containing element's type.
    pub owner: String,
    /// The property name after the dot.
    pub name: String,
    /// The values inside the property element (elements and/or text).
    pub values: Vec<XamlChild>,
    pub pos: TextPos,
}

impl XamlPropertyElement {
    /// The single element value, if there is exactly one element child.
    pub fn single_element(&self) -> Option<&XamlNode> {
        let mut iter = self.values.iter().filter_map(XamlChild::as_element);
        let first = iter.next()?;
        if iter.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    pub fn elements(&self) -> impl Iterator<Item = &XamlNode> {
        self.values.iter().filter_map(XamlChild::as_element)
    }
}
