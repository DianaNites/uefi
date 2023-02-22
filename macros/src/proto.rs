use nuuid::Uuid;
use proc_macro::TokenStream;
use quote::{__private::Span, format_ident, quote};
use syn::{
    parse_macro_input,
    spanned::Spanned,
    AttributeArgs,
    DeriveInput,
    Error,
    Expr,
    ExprArray,
    Ident,
    ItemFn,
    ItemStruct,
    Lit,
    Meta,
    NestedMeta,
    Pat,
    Type,
    TypeGroup,
    TypePath,
};

fn parse_args(
    args: &[NestedMeta],
    errors: &mut Vec<Error>,
    krate: &mut Ident,
    guid: &mut Option<String>,
) {
    for arg in args {
        match &arg {
            syn::NestedMeta::Meta(Meta::NameValue(m)) => {
                if let Some(i) = m.path.get_ident() {
                    if i == "crate" {
                        if let Lit::Str(s) = &m.lit {
                            *krate = format_ident!("{}", s.value());
                        } else {
                            errors.push(Error::new(m.lit.span(), "Expected string literal"));
                        }
                    } else {
                        errors.push(Error::new(m.span(), format!("Unexpected argument `{}`", i)));
                    }
                } else {
                    errors.push(Error::new(
                        m.span(),
                        format!("Unexpected argument `{:?}`", m.path),
                    ));
                }
            }
            // #[cfg(no)]
            syn::NestedMeta::Meta(m) => {
                let name = m.path().get_ident();
                let span = m.span();
                if let Some(name) = name {
                    errors.push(Error::new(span, format!("Unexpected argument `{}`", name)));
                } else {
                    errors.push(Error::new(span, format!("Unexpected argument `{:?}`", m)));
                }
            }
            syn::NestedMeta::Lit(Lit::Str(lit)) => {
                let s = lit.value();
                // Don't check for UUID validity here, its checked later.
                if guid.replace(s).is_some() {
                    errors.push(Error::new(lit.span(), "Duplicate GUID attribute"));
                }
            }
            syn::NestedMeta::Lit(l) => {
                errors.push(Error::new(l.span(), format!("Unknown literal: `{:?}`", l)));
            }
        }
    }
}

pub fn proto(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(input as ItemStruct);
    let mut errors: Vec<Error> = Vec::new();

    let mut krate = format_ident!("uefi");
    let mut guid: Option<String> = None;

    parse_args(&args, &mut errors, &mut krate, &mut guid);

    let imp_struct = &input.ident;
    let imp_generics = &input.generics;

    // This makes errors really nice
    let error_def = quote! {
        ()
    };

    // FIXME: Workaround the interface macro Type being `*mut Ty`
    let mut imp_raw_ty_ident = quote! {()};

    let mut match_path = |path: &syn::Path, span, errors: &mut Vec<Error>| {
        if let Some(path) = path.get_ident() {
            quote! { #path }
        } else {
            errors.push(Error::new(
                span,
                "Invalid type (1). This macro MUST only be used with `interface` types",
            ));
            error_def.clone()
        }
    };

    let mut match_group = |elem: &syn::Type, span, errors: &mut Vec<Error>| match elem {
        syn::Type::Path(TypePath { path, .. }) => match_path(path, span, errors),
        _ => {
            errors.push(Error::new(
                span,
                "Invalid type (4). This macro MUST only be used with `interface` types",
            ));
            error_def.clone()
        }
    };

    let mut match_ty = |ty: &Type, span| match ty {
        syn::Type::Path(TypePath { path, .. }) => match_path(path, span, &mut errors),

        syn::Type::Ptr(ptr) => match &*ptr.elem {
            syn::Type::Path(TypePath { path, .. }) => match_path(path, span, &mut errors),

            syn::Type::Group(TypeGroup { elem, .. }) => match_group(elem, span, &mut errors),

            _ => {
                errors.push(Error::new(
                    span,
                    "Invalid type (2). This macro MUST only be used with `interface` types",
                ));
                error_def.clone()
            }
        },
        _ => {
            errors.push(Error::new(
                span,
                "Invalid type (3). This macro MUST only be used with `interface` types",
            ));
            error_def.clone()
        }
    };

    let imp_first_field = match &input.fields {
        syn::Fields::Named(fields) => {
            if let Some(first) = fields.named.first() {
                let ty = &first.ty;
                let i = match_ty(ty, fields.named.span());
                imp_raw_ty_ident = quote! { #i };
                quote! { #ty }
            } else {
                errors.push(Error::new(fields.named.span(), "Missing Protocol GUID"));
                error_def
            }
        }
        syn::Fields::Unnamed(fields) => {
            if let Some(first) = fields.unnamed.first() {
                let ty = &first.ty;
                let i = match_ty(ty, fields.unnamed.span());
                imp_raw_ty_ident = quote! { #i };
                quote! { #ty }
            } else {
                errors.push(Error::new(fields.unnamed.span(), "Missing Protocol GUID"));
                error_def
            }
        }
        syn::Fields::Unit => {
            errors.push(Error::new(input.fields.span(), "Missing Protocol GUID"));
            error_def
        }
    };

    // This makes errors really nice
    let error_def = quote! {unsafe {
        #krate::proto::Guid::from_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ])
    }};

    let guid_bytes = if let Some(guid) = guid {
        match Uuid::parse_me(&guid) {
            Ok(guid) => {
                let lol = format!("{:?}", guid.to_bytes());
                if let Ok(lol) = syn::parse_str::<ExprArray>(&lol) {
                    quote! {unsafe {
                        #krate::proto::Guid::__from_bytes_protocol(#lol)
                    }}
                } else {
                    quote! {
                        compile_error!(
                            "Uh this shouldn't have happened. Syn failed when it shouldn't have.\n\
                            This breaks the macro.\n\
                            This is message brought to you by the Nuefi `Protocol` macro.\n\
                            Please direct your bug report there."
                        )

                        #error_def
                    }
                }
            }
            Err(e) => {
                // TODO: parse args config struct, store GUID lit span, use here.
                errors.push(Error::new(guid.span(), format!("Invalid GUID: {e}")));
                error_def
            }
        }
    } else {
        errors.push(Error::new(input.span(), "Missing Protocol GUID"));
        error_def
    };

    let expanded = quote! {
        #input

        // #[cfg(no)]
        unsafe impl<'table> #krate::proto::Protocol<'table> for #imp_struct #imp_generics {
            const GUID: #krate::proto::Guid = #guid_bytes;

            type Raw = #imp_raw_ty_ident;

            unsafe fn from_raw(this: #imp_first_field) -> Self {
                #imp_struct::new(this)
            }
        }
    };

    if let Some(e) = errors.into_iter().reduce(|mut acc, e| {
        acc.combine(e);
        acc
    }) {
        let e = e.into_compile_error();
        TokenStream::from(quote! {
            #e
            #expanded
        })
    } else {
        TokenStream::from(expanded)
    }
}
