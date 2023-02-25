#![allow(clippy::redundant_clone, clippy::ptr_arg)]
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, Parser},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute,
    AttributeArgs,
    Error,
    Ident,
    ItemFn,
    Lit,
    Meta,
    MetaList,
    MetaNameValue,
    NestedMeta,
    Pat,
    Path,
    Token,
};

type Args = Punctuated<NestedMeta, Token![,]>;

/// Options our macro accepts
struct Config {
    /// `nuefi` crate name
    ///
    /// `entry(crate = "name")`
    krate: Option<Ident>,

    /// Exit to fw delay in seconds
    ///
    /// `entry(delay(30))`
    delay: Option<u64>,

    /// Register global alloc
    ///
    /// `entry(alloc)`
    alloc: bool,

    /// Default panic handler
    ///
    /// `entry(panic)`
    panic: bool,

    /// Default alloc error handler
    ///
    /// `entry(alloc_error)`
    alloc_error: bool,

    /// Whether to generate and register a default `UefiLogger`
    ///
    /// - `entry(log)`
    /// - `entry(log(..))`
    log: Option<Log>,
}

impl Config {
    fn new() -> Self {
        Self {
            krate: None,
            delay: None,
            alloc: false,
            panic: false,
            alloc_error: false,
            log: None,
        }
    }
}

/// `entry(log(..))` options
struct Log {
    /// Whether logging is colorful or not
    color: bool,

    /// Whether all targets are enabled
    ///
    /// Mutually exclusive with `targets`
    all: bool,

    /// Enable just these targets
    ///
    /// Mutually exclusive with `all`
    targets: Option<Vec<String>>,

    /// Exclude these targets
    exclude: Option<Vec<String>>,
}

impl Log {
    fn new() -> Self {
        Self {
            //
            color: false,
            all: false,
            targets: None,
            exclude: None,
        }
    }
}

fn krate(i: &Ident, meta: &MetaNameValue, errors: &mut Vec<Error>, opts: &mut Config) -> bool {
    if i == "crate" {
        if let Lit::Str(s) = &meta.lit {
            match opts.krate {
                Some(_) => errors.push(Error::new(meta.span(), "Duplicate attribute `crate`")),
                None => {
                    opts.krate.replace(format_ident!("{}", s.value()));
                }
            }
        } else {
            errors.push(Error::new(meta.lit.span(), "Expected string literal"));
        }
        true
    } else {
        false
    }
}

fn delay(i: &Ident, list: &MetaList, errors: &mut Vec<Error>, opts: &mut Config) -> bool {
    if i == "delay" {
        if let Some(f) = list.nested.first() {
            match f {
                syn::NestedMeta::Meta(m) => {
                    errors.push(Error::new(
                        list.span(),
                        format!("Expected value: {:?}", list.nested),
                    ));
                }
                syn::NestedMeta::Lit(li) => match li {
                    Lit::Int(lit) => {
                        if let Ok(lit) = lit.base10_parse::<u64>() {
                            if opts.delay.replace(lit).is_some() {
                                errors.push(Error::new(list.span(), "Duplicate attribute `delay`"));
                            }
                        }
                    }
                    v => {
                        errors.push(Error::new(
                            li.span(),
                            format!("Expected integer, got: {:?}", f),
                        ));
                    }
                },
            }
        } else {
            errors.push(Error::new(
                list.span(),
                format!("Expected value: {:?}", list.nested),
            ));
        }
        true
    } else {
        false
    }
}

fn log(i: &Ident, list: &MetaList, errors: &mut Vec<Error>, opts: &mut Config) -> bool {
    if i == "log" {
        let mut log = Log::new();
        let mut exclude: Vec<String> = Vec::new();

        for a in &list.nested {
            match a {
                NestedMeta::Meta(Meta::Path(p)) => {
                    if let Some(i) = p.get_ident() {
                        if i == "color" {
                            if log.color {
                                errors.push(Error::new(p.span(), "Duplicate attribute `color`"));
                            }
                            log.color = true;
                        } else if i == "all" {
                        } else {
                            errors
                                .push(Error::new(i.span(), format!("Unexpected argument `{}`", i)));
                        }
                    }
                }
                NestedMeta::Meta(Meta::List(li)) => {
                    if let Some(i) = li.path.get_ident() {
                        if i == "exclude" {
                            if log.exclude.is_some() {
                                errors.push(Error::new(
                                    li.path.span(),
                                    "Duplicate attribute `exclude`",
                                ));
                            } else {
                                log.exclude.insert(exclude.clone()).push(String::new());
                            }
                        } else {
                            errors
                                .push(Error::new(i.span(), format!("Unexpected argument `{}`", i)));
                        }
                    }
                }
                // NestedMeta::Lit(_) => {}
                NestedMeta::Meta(m) => {
                    let path = m.path();
                    let span = path.span();
                    if let Some(i) = path.get_ident() {
                        errors.push(Error::new(m.span(), format!("Unexpected argument `{}`", i)));
                    } else {
                        errors.push(Error::new(
                            m.span(),
                            format!("Unexpected argument `{:?}`", path),
                        ));
                    }
                }
                e => {
                    errors.push(Error::new(
                        e.span(),
                        format!("Unexpected argument `{:?}`", e),
                    ));
                }
            }
        }

        if opts.log.replace(log).is_some() {
            errors.push(Error::new(i.span(), "Duplicate attribute `log`"));
        }

        true
    } else {
        false
    }
}

// TODO: Do this but the other way around, got value when didn't expect,
// try removing (args)
fn unexpected_as_path(i: &Ident, path: &Path, errors: &mut Vec<Error>, opts: &mut Config) -> bool {
    if i == "delay" {
        errors.push(Error::new(
            path.span(),
            "Attribute `delay` expected value. Try `delay(VALUE)`",
        ));
        true
    } else {
        false
    }
}

fn simple_opts(i: &Ident, path: &Path, errors: &mut Vec<Error>, opts: &mut Config) -> bool {
    if i == "log" {
        let log = Log::new();
        if opts.log.replace(log).is_some() {
            errors.push(Error::new(path.span(), "Duplicate attribute `log`"));
        }
        true
    } else if i == "alloc" {
        if opts.alloc {
            errors.push(Error::new(path.span(), "Duplicate attribute `alloc`"));
        }
        opts.alloc = true;
        true
    } else if i == "alloc_error" {
        if opts.alloc_error {
            errors.push(Error::new(path.span(), "Duplicate attribute `alloc_error`"));
        }
        opts.alloc_error = true;
        true
    } else if i == "panic" {
        if opts.panic {
            errors.push(Error::new(path.span(), "Duplicate attribute `panic`"));
        }
        opts.panic = true;
        true
    } else {
        false
    }
}

#[allow(clippy::if_same_then_else)]
fn parse_args(args: &[NestedMeta], errors: &mut Vec<Error>, opts: &mut Config) {
    for arg in args {
        match &arg {
            syn::NestedMeta::Meta(Meta::NameValue(m)) => {
                if let Some(i) = m.path.get_ident() {
                    if krate(i, m, errors, opts) {
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
            syn::NestedMeta::Meta(Meta::List(l)) => {
                if let Some(i) = l.path.get_ident() {
                    if delay(i, l, errors, opts) {
                        //
                    } else if log(i, l, errors, opts) {
                    } else {
                        errors.push(Error::new(
                            l.span(),
                            format!("Unexpected argument `{:?}`", l.path),
                        ));
                    }
                } else if let Some(i) = l.path.get_ident() {
                    errors.push(Error::new(l.span(), format!("Unexpected argument `{}`", i)));
                } else {
                    errors.push(Error::new(
                        l.span(),
                        format!("Unexpected argument `{:?}`", l.path),
                    ));
                }
            }
            syn::NestedMeta::Meta(m @ Meta::Path(p)) => {
                if let Some(i) = p.get_ident() {
                    if simple_opts(i, p, errors, opts) {
                    } else if unexpected_as_path(i, p, errors, opts) {
                    } else {
                        errors.push(Error::new(p.span(), format!("Unexpected argument `{}`", i)));
                    }
                } else {
                    errors.push(Error::new(
                        p.span(),
                        format!("Unexpected argument `{:?}`", p),
                    ));
                }
            }
            #[cfg(no)]
            syn::NestedMeta::Meta(m) => {
                let name = m.path().get_ident();
                let span = m.span();
                if let Some(name) = name {
                    errors.push(Error::new(span, format!("Unexpected argument `{}`", name)));
                } else {
                    errors.push(Error::new(span, format!("Unexpected argument `{:?}`", m)));
                }
            }
            syn::NestedMeta::Lit(l) => {
                errors.push(Error::new(l.span(), format!("Unknown literal: `{:?}`", l)));
            }
        }
    }
}

pub fn entry(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(input as ItemFn);
    let mut errors = Vec::new();

    let mut cfg = Config::new();

    parse_args(&args, &mut errors, &mut cfg);

    let mut krate: Option<Ident> = cfg.krate;
    let mut exit_prompt = false;
    let mut handle_log = false;
    let mut delay: Option<u64> = cfg.delay;
    let mut handle_alloc_error = cfg.alloc_error;
    let mut handle_alloc = cfg.alloc;
    let mut handle_panic = cfg.panic;

    let sig = &input.sig;
    let ident = &sig.ident;
    let attrs = &input.attrs;
    if !attrs.is_empty() {
        // panic!("Had {} attributes and expected zero", input.attrs.len());
    }
    let params = &sig.inputs;
    if params.is_empty() {
        errors.push(Error::new(
            sig.span(),
            format!("Incorrect function signature, expected `fn(handle: EfiHandle, table: SystemTable<Boot>) -> error::Result<()>`\
\n\
Try `fn {}(handle: EfiHandle, table: SystemTable<Boot>) -> error::Result<()>`
",ident),
        ));
    }
    if params.len() == 1 {
        let mut p = params.iter();
        let unexpected = p.next().unwrap();
        let span = unexpected.span();
        let err = Error::new(span, "Missing `table` argument");
        errors.push(err);
    }
    if params.len() > 2 {
        let p = params.iter().skip(2);
        for unexpected in p {
            let span = unexpected.span();
            match unexpected {
                syn::FnArg::Receiver(_) => errors.push(Error::new(span, "Unexpected argument")),
                syn::FnArg::Typed(n) => {
                    if let Pat::Ident(i) = &*n.pat {
                        errors.push(Error::new(
                            span,
                            format!("Unexpected argument: `{}`", i.ident),
                        ));
                    } else {
                        errors.push(Error::new(span, "Unexpected argument"))
                    }
                }
            }
        }
    }

    for a in params.iter().take(2) {
        match a {
            syn::FnArg::Receiver(a) => {
                errors.push(Error::new(a.span(), "Cannot be a method"));
            }
            syn::FnArg::Typed(_) => {
                // NOTE: Apparently not possible to verify types in proc macro?
            }
        };
    }

    // NOTE: Keep `MainCheck` up with actual definition.
    // This is breaking to change.
    let chk = quote! {
        type MainCheck = fn(handle: EfiHandle, table: SystemTable<Boot>) -> error::Result<()>;

        const _chk: MainCheck = #ident;
    };

    let exit_dur = if let Some(d) = delay {
        quote! {
            Some(#d)
        }
    } else {
        quote! {
            None
        }
    };

    let krate = krate.unwrap_or(format_ident!("nuefi"));

    let panic = if handle_panic {
        quote! {
            const _: () = {
                use #krate::handlers::panic;
                use core::panic::PanicInfo;

                // Helps with faulty rust-analyzer/linking errors
                #[cfg_attr(not(test), panic_handler)]
                fn handle_panic(info: &PanicInfo) -> ! {
                    panic(info);
                }
            };
        }
    } else {
        quote! {}
    };

    let alloc_error = if handle_alloc_error {
        quote! {
            const _: () = {
                use #krate::handlers::alloc_error;
                use core::alloc::Layout;

                // Helps with faulty rust-analyzer/linking errors
                #[cfg_attr(not(test), alloc_error_handler)]
                fn handle_alloc(layout: core::alloc::Layout) -> ! {
                    alloc_error(layout);
                }
            };
        }
    } else {
        quote! {}
    };

    let alloc = if handle_alloc {
        quote! {
            const _: () = {
                use #krate::mem::UefiAlloc;

                #[global_allocator]
                static NUEFI_ALLOC: UefiAlloc = UefiAlloc::new();
            };
        }
    } else {
        quote! {}
    };

    let log = if handle_log {
        quote! {
            const _: () = {
                use #krate::logger::{UefiColorLogger, UefiLogger};
                // use ::core::module_path;

                static NUEFI_LOGGER: UefiColorLogger = UefiLogger::new(&[module_path!(), "nuefi"])
                    .exclude(&["nuefi::mem"])
                    .color();
            };
        }
    } else {
        quote! {}
    };

    // NOTE: Macro can/should/MUST do linker hacks to
    // ensure persistent runtime panic/alloc_error hooks
    // that way we can allow them to be overridden, and free boot memory.
    // Suspect just need link_section

    let expanded = quote! {
        const _: () = {
            use #krate::{
                EfiHandle,
                SystemTable,
                table::Boot,
                error,
            };

            #chk

            #[no_mangle]
            pub static __INTERNAL_NUEFI_YOU_MUST_USE_MACRO: Option<bool> = Some(false);

            #[no_mangle]
            pub static __INTERNAL_NUEFI_EXIT_DURATION: Option<u64> = #exit_dur;

            #[no_mangle]
            pub fn __internal__nuefi__main(handle: EfiHandle, table: SystemTable<Boot>) -> error::Result<()> {
                #ident(handle, table)
            }
        };

        #input

        #panic

        #alloc

        #alloc_error
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
