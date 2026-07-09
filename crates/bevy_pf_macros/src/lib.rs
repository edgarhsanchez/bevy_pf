//! Proc macros for `bevy_pf`.
//!
//! - [`xaml!`]: inline XAML validated at compile time.
//! - [`include_xaml!`]: load a `.xaml` file at compile time, validated.
//!
//! Both macros parse the XAML with `bevy_pf_xaml` during compilation, so
//! malformed markup is a build error, and expand to code that hands the
//! validated source to the `bevy_pf` runtime.

use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, parse_macro_input};

/// Validate and embed a XAML string literal at compile time.
///
/// ```ignore
/// let doc: XamlScene = xaml!(r#"
///     <StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
///         <Button Content="Click me"/>
///     </StackPanel>
/// "#);
/// ```
#[proc_macro]
pub fn xaml(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let source = lit.value();
    expand_xaml_source(&source, lit.span())
}

/// Validate and embed an external `.xaml` file at compile time.
///
/// The path is resolved relative to the crate root (`CARGO_MANIFEST_DIR`).
/// The file is re-read on rebuild, and a change to it invalidates the build.
///
/// ```ignore
/// let doc: XamlScene = include_xaml!("assets/main_window.xaml");
/// ```
#[proc_macro]
pub fn include_xaml(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let rel = lit.value();

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => d,
        Err(_) => {
            return syn::Error::new(lit.span(), "CARGO_MANIFEST_DIR is not set")
                .to_compile_error()
                .into();
        }
    };
    let path = std::path::Path::new(&manifest_dir).join(&rel);
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return syn::Error::new(
                lit.span(),
                format!("cannot read `{}`: {e}", path.display()),
            )
            .to_compile_error()
            .into();
        }
    };

    let path_str = path.to_string_lossy();
    let validated = expand_xaml_source(&source, lit.span());
    let validated = proc_macro2::TokenStream::from(validated);
    // `include_str!` of the absolute path makes the compiler track the file
    // so edits to it trigger rebuilds.
    quote! {{
        const _: &str = include_str!(#path_str);
        #validated
    }}
    .into()
}

fn expand_xaml_source(source: &str, span: proc_macro2::Span) -> TokenStream {
    match bevy_pf_xaml::parse(source) {
        Ok(_) => {
            // The XAML is valid. Expand to a lazily-parsed scene handle; the
            // runtime re-parses the embedded source (cheap, and keeps the
            // macro decoupled from bevy types).
            quote! {
                ::bevy_pf::XamlScene::from_static_validated(#source)
            }
            .into()
        }
        Err(e) => syn::Error::new(span, format!("invalid XAML: {e}"))
            .to_compile_error()
            .into(),
    }
}
