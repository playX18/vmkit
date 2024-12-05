use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse_quote, punctuated::Punctuated, token::Paren, Expr, FnArg, Generics, Ident, PatType, Token, Type
};
use synstructure::decl_derive;

enum Mode {
    NoTrace,
    ScanSlots,
    TraceFields,
}

fn find_trace_meta(attrs: &[syn::Attribute]) -> syn::Result<Option<&syn::Attribute>> {
    let mut found = None;
    for attr in attrs {
        if attr.path().is_ident("managed") && found.replace(attr).is_some() {
            return Err(syn::parse::Error::new_spanned(
                attr.path(),
                "Cannot specify multiple `#[managed]` attributes! Consider merging them.",
            ));
        }
    }

    Ok(found)
}

fn find_tagged_meta(attrs: &[syn::Attribute]) -> syn::Result<Option<&syn::Attribute>> {
    let mut found = None;
    for attr in attrs {
        if attr.path().is_ident("tagged") && found.replace(attr).is_some() {
            return Err(syn::parse::Error::new_spanned(
                attr.path(),
                "Cannot specify multiple `#[tagged]` attributes! Consider merging them.",
            ));
        }
    }

    Ok(found)
}


fn find_arraylike_meta(attrs: &[syn::Attribute]) -> syn::Result<Option<&syn::Attribute>> {
    let mut found = None;
    for attr in attrs {
        if attr.path().is_ident("arraylike") && found.replace(attr).is_some() {
            return Err(syn::parse::Error::new_spanned(
                attr.path(),
                "Cannot specify multiple `#[arraylike]` attributes! Consider merging them.",
            ));
        }
    }

    Ok(found)
}

fn usage_error(meta: &syn::meta::ParseNestedMeta, msg: &str) -> syn::parse::Error {
    meta.error(format_args!(
        "{msg}. `#[managed(...)]` requires one mode (`no_trace`, `scan_slots`, `trace_fields`)."
    ))
}

struct ArrayLike {
    len_field: Option<Ident>,
    // A getter for length (if `len` field is not available or is encoded in some way).
    len_getter: Option<Expr>,
    // A setter for length (if `len` field is not available or is encoded in some way).
    len_setter: Option<Expr>,
    data_field: Option<Ident>,
    data_type: Option<Type>,
    trace: bool,
}

impl Default for ArrayLike {
    fn default() -> Self {
        Self {
            len_field: None,
            len_getter: None,
            len_setter: None,
            data_field: None,
            data_type: None,
            trace: true,
        }
    }
}


fn trace_derive(s: synstructure::Structure) -> TokenStream {
    let mut mode = None;
    let mut runtime = None;
    let mut vtable_name = None;
    let mut tagged_mask = None;
    let mut tagged_tag = None;
    let result = match find_trace_meta(&s.ast().attrs) {
        Ok(Some(attr)) => attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("runtime") {
                let rt: syn::Type = meta.value()?.parse()?;
                runtime = Some(rt);
                return Ok(());
            }

            if meta.path.is_ident("vtable") {
                let vt: syn::Ident = meta.value()?.parse()?;
                vtable_name = Some(vt);
                return Ok(());
            }

            meta.input.parse::<syn::parse::Nothing>()?;
            if meta.path.is_ident("trace_mode") {
                let mode_id = meta.value()?.parse::<syn::Ident>()?;
                if mode.is_some() {
                    return Err(usage_error(&meta, "multiple modes specified"));
                } else if mode_id == "none" {
                    mode = Some(Mode::NoTrace);
                } else if mode_id == "scan_slots" {
                    mode = Some(Mode::ScanSlots);
                } else if mode_id == "trace_fields" {
                    mode = Some(Mode::TraceFields);
                } else {
                    return Err(usage_error(&meta, "unknown option"));
                }
            }
            Ok(())
        }),
        Ok(None) => Ok(()),
        Err(err) => Err(err),
    };

    if let Err(err) = result {
        return err.to_compile_error();
    }

    match find_tagged_meta(&s.ast().attrs) {
        Ok(Some(attr)) => {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("tag") {
                    tagged_tag = Some(meta.value()?.parse::<syn::Expr>()?);
                } else if meta.path.is_ident("mask") {
                    tagged_mask = Some(meta.value()?.parse::<syn::Expr>()?);
                }
                Ok(())
            }).unwrap();
        }
        Ok(_) => (),
        Err(err) => return err.to_compile_error()
    }


    let Some(mode) = mode else {
        panic!(
            "{}",
            "deriving `Managed` requires a `#[managed(...)]` attribute"
        );
    };
    let rt = runtime.expect("deriving `Managed` requires a #[trace(runtime=Name)]` attribute");

    let mut arraylike: Option<ArrayLike> = None;

    let result = match find_arraylike_meta(&s.ast().attrs) {
        Ok(Some(attr)) => attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("no_trace") {
                if arraylike.is_none() {
                    arraylike = Some(ArrayLike::default())
                }

                arraylike.as_mut().unwrap().trace = false;
            }
            if meta.path.is_ident("len") {
                if let Some(_) = arraylike.as_ref().filter(|x| x.len_field.is_some()) {
                    return Err(usage_error(&meta, "multiple len fields specified"));
                }

                let id: syn::Ident = meta.value()?.parse()?;
                if arraylike.is_none() {
                    arraylike = Some(Default::default())
                }

                arraylike.as_mut().unwrap().len_field = Some(id);
            } else if meta.path.is_ident("data_type") {
                if let Some(_) = arraylike.as_ref().filter(|x| x.data_type.is_some()) {
                    return Err(usage_error(&meta, "multiple data types specified"));
                }

                let ty: syn::Type = meta.value()?.parse()?;
                if arraylike.is_none() {
                    arraylike = Some(Default::default())
                }

                arraylike.as_mut().unwrap().data_type = Some(ty);
            } else if meta.path.is_ident("data") {
                if let Some(_) = arraylike.as_ref().filter(|x| x.data_field.is_some()) {
                    return Err(usage_error(&meta, "multiple data fields specified"));
                }

                let ty: syn::Ident = meta.value()?.parse()?;
                if arraylike.is_none() {
                    arraylike = Some(Default::default())
                }

                arraylike.as_mut().unwrap().data_field = Some(ty);
            } else if meta.path.is_ident("len_getter") {
                if let Some(_) = arraylike.as_ref().filter(|x| x.len_getter.is_some()) {
                    return Err(usage_error(&meta, "multiple data fields specified"));
                }

                let ty: syn::Expr = meta.value()?.parse()?;
                if arraylike.is_none() {
                    arraylike = Some(Default::default())
                }

                arraylike.as_mut().unwrap().len_getter = Some(ty);
            }  else if meta.path.is_ident("len_setter") {
                if let Some(_) = arraylike.as_ref().filter(|x| x.len_setter.is_some()) {
                    return Err(usage_error(&meta, "multiple data fields specified"));
                }

                let ty: syn::Expr = meta.value()?.parse()?;
                if arraylike.is_none() {
                    arraylike = Some(Default::default())
                }

                arraylike.as_mut().unwrap().len_setter = Some(ty);
            }

            Ok(())
        }),

        Ok(None) => Ok(()),
        Err(err) => Err(err),
    };
    if let Err(err) = result {
        return err.to_compile_error();
    }

    if let Some(a) = arraylike.as_ref() {
        if (a.len_field.is_none() && a.len_getter.is_none())
            || a.data_field.is_none()
            || a.data_type.is_none()
        {
            panic!(
                "Deriving 'Managed` with #[arraylike] requires len, data and data_type specified"
            )
        }
    }

    let mut trace_body = TokenStream::new();
    let trace_impl = if !matches!(mode, Mode::NoTrace) {
        s.each(|bi| {
            if let Some(name) = bi.ast().ident.as_ref() {
                if let Some(alike) = arraylike.as_ref() {
                    if let Some(adata) = alike.data_field.as_ref() {
                        if name.to_string() == adata.to_string() {
                            return quote! { let _ = #bi; }; // skip
                        }
                    }
                }
            }

            if bi
                .ast()
                .attrs
                .iter()
                .any(|attr| attr.meta.path().is_ident("skip"))
            {
                return quote! {let _ = #bi;};
            }

            match mode {
                Mode::NoTrace => unreachable!(),
                Mode::ScanSlots => {
                    quote! {
                        let bi = #bi;
                        ::vmkit::objectmodel::traits::ScanSlots::<#rt>::scan(bi, visitor);
                    }
                }

                Mode::TraceFields => {
                    quote! {
                        let bi = #bi;
                        ::vmkit::objectmodel::traits::TraceRefs::<#rt>::trace(bi, tracer);
                    }
                }
            }
        })
        .to_tokens(&mut trace_body);

        let mut trace_body = quote! {
            match *self {
                #trace_body
            }
        };

        let arraylike_trace = if let Some(arraylike) = arraylike.as_ref() {
            let len_field = arraylike.len_field.as_ref();
            let len_getter = arraylike.len_getter.as_ref();
            let len = match (len_field, len_getter) {
                (Some(field), None) => quote! { self.#field },
                (_, Some(getter)) => {
                    quote! { { (#getter)(self) } }
                }
                _ => panic!("no length field getter declared for arraylike"),
            };
            let ty = arraylike.data_type.as_ref().unwrap();
            let data = arraylike.data_field.as_ref().unwrap();

            match mode {
                Mode::TraceFields if arraylike.trace => {
                    quote! {
                        let this = self;
                        let len = ::vmkit::objectmodel::traits::ArrayLikeLength::arraylike_length(&(#len));
                        for i in 0..len {
                            unsafe {
                                let data: &mut #ty = self.#data.as_mut_ptr().cast::<#ty>().add(i).as_mut().unwrap();
                                ::vmkit::objectmodel::traits::TraceRefs::trace(data, tracer);
                            }
                        }
                    }
                }

                Mode::ScanSlots if arraylike.trace => {
                    quote! {
                        let this = self;
                        let len = ::vmkit::objectmodel::traits::ArrayLikeLength::arraylike_length(&(#len));
                        for i in 0..len {
                            unsafe {
                                let data: &#ty = self.#data.as_ptr().cast::<#ty>().add(i).as_ref().unwrap();
                                ::vmkit::objectmodel::traits::ScanSlots::scan(data, visitor);
                            }
                        }
                    }
                }

                _ => quote! {},
            }
        } else {
            quote! {}
        };

        arraylike_trace.to_tokens(&mut trace_body);
        match mode {
            Mode::ScanSlots => s.clone().gen_impl(quote! {
                gen impl ::vmkit::objectmodel::traits::ScanSlots<#rt> for @Self {
                    fn scan(&self, visitor: &mut ::vmkit::mm::scanning::Visitor<#rt>) {
                        #trace_body
                    }
                }
            }),

            Mode::TraceFields => s.clone().gen_impl(quote! {
                gen impl ::vmkit::objectmodel::traits::TraceFields<#rt> for @Self {
                    fn trace(&mut self, tracer: &mut ::vmkit::mm::scanning::Tracer<#rt>) {
                        #trace_body
                    }
                }
            }),

            _ => unreachable!(),
        }
    } else {
        quote! {}
    };
    let t = s.ast().ident.clone();

    let (impl_generics, ty_generics, where_clauses) = s.ast().generics.split_for_impl();
    let turbofish = ty_generics.as_turbofish();

    let (size, compute_size) = if arraylike.is_none() {
        (
            quote! { ::core::mem::size_of::<#t #ty_generics>() },
            quote! { None },
        )
    } else {
        let arraylike = arraylike.as_ref().unwrap();
        let len_field = arraylike.len_field.as_ref();
        let len_getter = arraylike.len_getter.as_ref();
        let len = match (len_field, len_getter) {
            (Some(field), None) => quote! { this.#field },
            (_, Some(getter)) => {
                quote! { { (#getter)(this) } }
            }
            _ => panic!("no length field getter declared for arraylike"),
        };

        let data_type = arraylike.data_type.as_ref().unwrap();
        (
            quote! { 0 },
            quote! {
                {
                    extern "C" fn compute_size #impl_generics (objref: ::vmkit::mmtk::util::ObjectReference) -> ::core::num::NonZeroUsize
                    #where_clauses    {
                        let addr = objref.to_raw_address();
                        let this = unsafe { addr.as_ref::<#t #ty_generics>() };

                        ::core::num::NonZeroUsize::new(size_of::<#t #ty_generics>() + (#len as usize * ::core::mem::size_of::<#data_type>())).unwrap()
                    }

                    Some(compute_size #turbofish)
                }
            },
        )
    };

    let trace_callback = match mode {
        Mode::NoTrace => quote!(::vmkit::objectmodel::vtable::TraceCallback::NoTrace),
        Mode::ScanSlots => {
            quote! {
                {
                    fn scan_slots #impl_generics (objref: ::vmkit::mmtk::util::ObjectReference, visitor: &mut ::vmkit::mm::scanning::Visitor<#rt>)
                    #where_clauses {
                        let addr = objref.to_raw_address();
                        let value = unsafe { addr.as_ref::<#t #ty_generics>() };

                        ::vmkit::objectmodel::traits::ScanSlots::scan(value, visitor);
                    }

                    ::vmkit::objectmodel::vtable::TraceCallback::ScanSlots(scan_slots #turbofish)
                }
            }
        }
        Mode::TraceFields => {
            quote! {
                {
                    fn trace_fields #impl_generics (objref: ::vmkit::mmtk::util::ObjectReference, visitor: &mut ::vmkit::mm::scanning::Tracer<#rt>)
                    #where_clauses {
                        let addr = objref.to_raw_address();
                        let value = unsafe { value.as_mut::<#t #ty_generics>() };

                        ::vmkit::objectmodel::traits::TraceRefs::trace(value, visitor);
                    }

                    ::vmkit::objectmodel::vtable::TraceCallback::TraceFields(trace_fields #turbofish)
                }
            }
        }
    };

    let vtable_def = quote! {
        impl #impl_generics ::vmkit::objectmodel::traits::VTableDef<#rt> for #t #ty_generics
        #where_clauses
        {
            const VTABLE: ::vmkit::objectmodel::vtable::GCVTable<#rt> = ::vmkit::objectmodel::vtable::GCVTable::<#rt> {
                magic: 0xff57ab1eff57ab1e,
                size: #size,
                compute_size: #compute_size,
                trace: #trace_callback,
                finalize: ::vmkit::objectmodel::vtable::FinalizeCallback::None,
                alignment: unsafe { ::core::num::NonZeroUsize::new_unchecked(::core::mem::align_of::<usize>()) },
            };
        }
    };

    // a constructor for the managed type. For arraylike we generate multiple constructors
    let mut impl_body = TokenStream::new();

    // No arraylike -> simply add `new()` method.
    let ctor_ret: syn::Type = match (&tagged_mask, &tagged_tag) {
        (Some(_), Some(_)) => parse_quote!{ ::vmkit::mmtk::util::Address },
        _ => parse_quote!{::vmkit::mmtk::util::ObjectReference}
    };
    let mut sig = syn::Signature {
        abi: None,
        asyncness: None,
        constness: None,
        fn_token: Token![fn](Span::call_site()),
        unsafety: None,
        ident: Ident::new("new", Span::call_site()),
        generics: Generics::default(),
        paren_token: Paren {
            ..Default::default()
        },
        inputs: Punctuated::new(),
        variadic: None,
        output: syn::ReturnType::Type(
            Token![->](Span::call_site()),
            Box::new(ctor_ret),
        ),
    };



    let ctor_ret_expr = match (&tagged_mask, &tagged_tag) {
       (Some(ref _mask), Some(ref tag)) => {
           quote! { unsafe { Address::from_usize(objref.to_raw_address() | (#tag)) } }
       }

       _ => quote! { objref }
    };

    let ctor = if arraylike.is_none() {
        let mut init: syn::ExprStruct = parse_quote!(Self {});

        s.each(|bi| {
            let id = bi.ast().ident.clone().unwrap();
            let ty = bi.ast().ty.clone();
            init.fields.push(parse_quote! { #id });

            sig.inputs.push(FnArg::Typed(PatType {
                attrs: vec![],
                pat: Box::new(parse_quote! { #id }),
                colon_token: Token![:](Span::call_site()),
                ty: Box::new(ty),
            }));
            quote! {}
        });

        quote! {
            pub #sig {
                use vmkit::objectmodel::traits::VTableDef;
                let objref = ::vmkit::mm::vmkit_allocate::<#rt>(
                    unsafe { std::mem::transmute(::vmkit::runtime::threads::vmkit_current_thread()) },
                    size_of::<#t #ty_generics>(),
                    unsafe { std::mem::transmute(&#t #turbofish::VTABLE) }
                );
                unsafe {
                    use ::vmkit::runtime::Runtime;
                    if #rt::VO_BIT {
                        println!("SET VO-BIT!!!");
                        ::vmkit::mm::vmkit_set_vo_bit::<#rt>(objref);

                    }
                    objref.to_raw_address().store(#init);
                }

                #ctor_ret_expr
            }
        }
    } else {
        // new(size: usize, init: T) -> Address/ObjectReference
        let al = arraylike.as_ref().unwrap();
        let data = al.data_field.as_ref().unwrap();
        let data_type = al.data_type.as_ref().unwrap();
        let len_field = al.len_field.as_ref();
        let mut init: syn::ExprStruct = parse_quote!(Self {
            #data: [],
        });


        s.each(|bi| {
            let id = bi.ast().ident.clone().unwrap();
            if Some(&id) == len_field {
                init.fields.push(parse_quote! { #id: array_size as _ });
                return quote!{}
            }
            if &id == data {
                return quote!{};
            }
            let ty = bi.ast().ty.clone();
            init.fields.push(parse_quote! { #id });

            sig.inputs.push(FnArg::Typed(PatType {
                attrs: vec![],
                pat: Box::new(parse_quote! { #id }),
                colon_token: Token![:](Span::call_site()),
                ty: Box::new(ty),
            }));
            quote! {}
        });

        sig.inputs.push(FnArg::Typed(PatType {
            attrs: vec![],
            pat: Box::new(parse_quote! { array_size }),
            colon_token: Token![:](Span::call_site()),
            ty: Box::new(parse_quote!(usize))
        }));
        sig.inputs.push(FnArg::Typed(PatType {
            attrs: vec![],
            pat: Box::new(parse_quote! { array_init }),
            colon_token: Token![:](Span::call_site()),
            ty: Box::new(data_type.clone())
        }));

        sig.generics.where_clause = Some(parse_quote! {
            where #data_type: Clone 
        });

        let (len_init, len_access) = match (&al.len_field, &al.len_setter, &al.len_getter) {
            (_, Some(setter), Some(getter)) => {
                (quote! { (#setter)(arr, array_size); }, quote! { (#getter)(arr) })
            }
            (Some(field), _, None) => {
                (quote! { arr.#field = array_size as _; }, quote! { arr.#field })
            }

            (Some(field), None, Some(getter)) => {
                (quote! {
                    arr.#field = array_size as _;
                }, quote! { (#getter)(arr) })
            }
            _ => unreachable!("no setter and no length field")
        };

        quote! {
            impl #impl_generics ::core::ops::Index<usize> for #t #ty_generics {
                type Output = #data_type;

                fn index(&self, idx: usize) -> &Self::Output {
                    let arr = self;
                    assert!(idx < #len_access);
                    unsafe {
                        arr.#data.as_ptr().cast::<#data_type>().add(idx).as_ref().unwrap_unchecked()
                    } 
                }
            }

            impl #impl_generics ::core::ops::IndexMut<usize> for #t #ty_generics {
                fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
                    let arr = self;
                    assert!(idx < #len_access);
                    unsafe {
                        arr.#data.as_mut_ptr().cast::<#data_type>().add(idx).as_mut().unwrap_unchecked()
                    } 
                }
            }

            impl #impl_generics ::core::convert::AsRef<[#data_type]> for #t #ty_generics {
                fn as_ref(&self) -> &[#data_type] {
                    unsafe {
                        let arr = self;
                        ::core::slice::from_raw_parts(arr.#data.as_ptr().cast::<#data_type>(), #len_access)
                    }
                }
            } 

            impl #impl_generics ::core::convert::AsMut<[#data_type]> for #t #ty_generics {
                fn as_mut(&mut self) -> &mut [#data_type] {
                    unsafe {
                        let arr = self;
                        ::core::slice::from_raw_parts_mut(arr.#data.as_mut_ptr().cast::<#data_type>(), #len_access)
                    }
                }
            } 

            impl #impl_generics ::core::ops::Deref for #t #ty_generics {
                type Target = [#data_type];

                fn deref(&self) -> &Self::Target {
                    unsafe {
                        let arr = self;
                        ::core::slice::from_raw_parts(arr.#data.as_ptr().cast::<#data_type>(), #len_access)
                    }
                }
            }

            impl #impl_generics ::core::ops::DerefMut for #t #ty_generics {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    unsafe {
                        let arr = self;
                        ::core::slice::from_raw_parts_mut(arr.#data.as_mut_ptr().cast::<#data_type>(), #len_access)
                    }
                }
            }

        }.to_tokens(&mut impl_body);

        quote! {
            pub #sig {
                use vmkit::objectmodel::traits::VTableDef;
                let objref = ::vmkit::mm::vmkit_allocate::<#rt>(
                    unsafe { std::mem::transmute(::vmkit::runtime::threads::vmkit_current_thread()) },
                    size_of::<#t #ty_generics>() + (array_size * size_of::<#data_type>()),
                    unsafe { std::mem::transmute(&#t #turbofish::VTABLE) }
                );
                unsafe {
                    use ::vmkit::runtime::Runtime;
                    if #rt::VO_BIT {
                        ::vmkit::mm::vmkit_set_vo_bit::<#rt>(objref);

                    }
                    objref.to_raw_address().store(#init);
                    let arr = objref.to_raw_address().as_mut_ref::<#t #ty_generics>();
                    #len_init

                    for i in 0..array_size {
                        arr.#data.as_mut_ptr().cast::<#data_type>().add(i).write(array_init.clone());
                    }
                }

                #ctor_ret_expr
            }
        }
    };

    quote! {
        impl #impl_generics #t #ty_generics {
            #ctor
        }
    }
    .to_tokens(&mut impl_body);

    quote! {
        #vtable_def
        #trace_impl
        #impl_body
    }
}

decl_derive!(
    [Managed, attributes(managed, arraylike, skip, tagged)] => trace_derive
);
