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
            return syn::Error::new(lit.span(), format!("cannot read `{}`: {e}", path.display()))
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

/// Derive equality-checked, per-property-notifying setters for a view-model
/// — the `INotifyPropertyChanged` boilerplate, generated.
///
/// On `struct GameVm { score: u32 }` this emits a `GameVmBindable` extension
/// trait implemented for `bevy_pf::Bindable`, with one setter per named
/// field:
///
/// ```ignore
/// #[derive(Reflect, Default, Bindable)]
/// struct GameVm { score: u32 }
///
/// vm.set_score(42);  // writes + notifies bindings reading `score`
/// vm.set_score(42);  // equal value: no write, no notify — returns false
/// ```
///
/// Setters route through `Bindable::update_named`, so only bindings whose
/// path overlaps the field re-apply. The model must be the ROOT type of the
/// `Bindable` (scoped views created with `Bindable::at` still work — the
/// notification path is joined with the scope). Requires each field to be
/// `PartialEq`. Tuple structs, enums, and generics are not supported.
#[proc_macro_derive(Bindable)]
pub fn derive_bindable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "#[derive(Bindable)] does not support generic view-models",
        )
        .to_compile_error()
        .into();
    }
    let syn::Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(
            name,
            "#[derive(Bindable)] only supports structs with named fields",
        )
        .to_compile_error()
        .into();
    };
    let syn::Fields::Named(fields) = &data.fields else {
        return syn::Error::new_spanned(
            name,
            "#[derive(Bindable)] only supports structs with named fields",
        )
        .to_compile_error()
        .into();
    };

    let trait_name = quote::format_ident!("{name}Bindable");
    let mut sigs = Vec::new();
    let mut impls = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let setter = quote::format_ident!("set_{ident}");
        let path = ident.to_string();
        let set_doc = format!(
            "Set `{path}`, notifying only bindings that read it. \
             Returns whether the value changed."
        );
        sigs.push(quote! {
            #[doc = #set_doc]
            fn #setter(&self, value: #ty) -> bool;
        });
        impls.push(quote! {
            fn #setter(&self, value: #ty) -> bool {
                self.update_named::<#name>(#path, |m| {
                    if m.#ident == value {
                        false
                    } else {
                        m.#ident = value;
                        true
                    }
                })
                .unwrap_or(false)
            }
        });
    }

    let vis = &input.vis;
    let trait_doc = format!("Generated setters binding [`{name}`] fields to `bevy_pf::Bindable`.");
    quote! {
        #[doc = #trait_doc]
        #vis trait #trait_name {
            #(#sigs)*
        }
        impl #trait_name for ::bevy_pf::Bindable {
            #(#impls)*
        }
    }
    .into()
}
