//! XAML parser and core value types for `bevy_pf`.
//!
//! This crate is deliberately free of any Bevy dependency so that it can be
//! used both by the `bevy_pf_macros` proc-macro crate (compile-time XAML
//! validation and code generation) and by `bevy_pf` itself (runtime `.xaml`
//! asset loading). It provides:
//!
//! - [`parse`]: XML + XAML-semantics parser producing a [`XamlDocument`] AST,
//!   with namespace handling, property-element syntax, attached properties,
//!   and markup extensions (`{Binding ...}`, `{StaticResource ...}`, ...).
//! - [`value`]: framework-agnostic WPF value types ([`value::Thickness`],
//!   [`value::GridLength`], [`value::PfColor`], ...) and the string type
//!   converters XAML requires ("Red" -> color, "1,2,3,4" -> thickness).

pub mod ast;
pub mod error;
pub mod geometry;
pub mod markup;
pub mod parser;
pub mod uri;
pub mod value;

pub use ast::*;
pub use error::{XamlError, XamlResult};
pub use markup::MarkupExtension;
pub use parser::parse;
