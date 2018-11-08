#![recursion_limit = "256"]
extern crate syn;

#[macro_use]
extern crate synstructure;
#[macro_use]
extern crate quote;
extern crate proc_macro2;

use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::{Data, Ident, Lit, LitBool, LitStr, Meta, MetaNameValue, NestedMeta};

#[derive(Debug, Clone, Copy)]
enum Trait {
    FromValue,
    ToValue,
    ProcessValue,
}

decl_derive!([ToValue, attributes(metastructure)] => process_to_value);
decl_derive!([FromValue, attributes(metastructure)] => process_from_value);
decl_derive!([ProcessValue, attributes(metastructure)] => process_process_value);

fn process_to_value(s: synstructure::Structure) -> TokenStream {
    process_metastructure_impl(s, Trait::ToValue)
}

fn process_from_value(s: synstructure::Structure) -> TokenStream {
    process_metastructure_impl(s, Trait::FromValue)
}

fn process_process_value(s: synstructure::Structure) -> TokenStream {
    process_metastructure_impl(s, Trait::ProcessValue)
}

fn process_wrapper_struct_derive(
    s: synstructure::Structure,
    t: Trait,
) -> Result<TokenStream, synstructure::Structure> {
    // The next few blocks are for finding out whether the given type is of the form:
    // struct Foo(Bar)  (tuple struct with a single field)

    if s.variants().len() != 1 {
        // We have more than one variant (e.g. `enum Foo { A, B }`)
        return Err(s);
    }

    if s.variants()[0].bindings().len() != 1 {
        // The single variant has multiple fields
        // e.g. `struct Foo(Bar, Baz)`
        //      `enum Foo { A(X, Y) }`
        return Err(s);
    }

    if let Some(_) = s.variants()[0].bindings()[0].ast().ident {
        // The variant has a name
        // e.g. `struct Foo { bar: Bar }` instead of `struct Foo(Bar)`
        return Err(s);
    }

    let name = &s.ast().ident;

    Ok(match t {
        Trait::FromValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;

                gen impl __processor::FromValue for @Self {
                    #[inline(always)]
                    fn from_value(
                        __value: __meta::Annotated<__meta::Value>,
                    ) -> __meta::Annotated<Self> {
                        match __processor::FromValue::from_value(__value) {
                            Annotated(Some(__value), __meta) => Annotated(Some(#name(__value)), __meta),
                            Annotated(None, __meta) => Annotated(None, __meta),
                        }
                    }
                }
            })
        }
        Trait::ToValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use types as __types;
                use meta as __meta;
                extern crate serde as __serde;

                gen impl __processor::ToValue for @Self {
                    #[inline(always)]
                    fn to_value(
                        mut __value: __meta::Annotated<Self>
                    ) -> __meta::Annotated<__meta::Value> {
                        let __value = __value.map_value(|x| x.0);
                        __processor::ToValue::to_value(__value)
                    }

                    #[inline(always)]
                    fn serialize_payload<S>(&self, __serializer: S) -> Result<S::Ok, S::Error>
                    where
                        Self: Sized,
                        S: __serde::ser::Serializer
                    {
                        __processor::ToValue::serialize_payload(&self.0, __serializer)
                    }

                    #[inline(always)]
                    fn extract_child_meta(&self) -> __meta::MetaMap
                    where
                        Self: Sized,
                    {
                        __processor::ToValue::extract_child_meta(&self.0)
                    }
                }
            })
        }
        Trait::ProcessValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;

                gen impl __processor::ProcessValue for @Self {
                    #[inline(always)]
                    fn process_value<P: __processor::Processor>(
                        __value: __meta::Annotated<Self>,
                        __processor: &P,
                        __state: __processor::ProcessingState
                    ) -> __meta::Annotated<Self> {
                        let __new_annotated = match __value {
                            __meta::Annotated(Some(__value), __meta) => {
                                __processor::ProcessValue::process_value(
                                    __meta::Annotated(Some(__value.0), __meta), __processor, __state)
                            }
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta)
                        };
                        match __new_annotated {
                            __meta::Annotated(Some(__value), __meta) => __meta::Annotated(Some(#name(__value)), __meta),
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta)
                        }
                    }
                }
            })
        }
    })
}

fn process_enum_struct_derive(
    s: synstructure::Structure,
    t: Trait,
) -> Result<TokenStream, synstructure::Structure> {
    if let Data::Enum(_) = s.ast().data {
    } else {
        return Err(s);
    }

    let mut process_func = None;
    let mut tag_key = "type".to_string();
    for attr in &s.ast().attrs {
        let meta = match attr.interpret_meta() {
            Some(meta) => meta,
            None => continue,
        };
        if meta.name() != "metastructure" {
            continue;
        }

        if let Meta::List(metalist) = meta {
            for nested_meta in metalist.nested {
                match nested_meta {
                    NestedMeta::Literal(..) => panic!("unexpected literal attribute"),
                    NestedMeta::Meta(meta) => match meta {
                        Meta::NameValue(MetaNameValue { ident, lit, .. }) => {
                            if ident == "process_func" {
                                match lit {
                                    Lit::Str(litstr) => {
                                        process_func = Some(litstr.value());
                                    }
                                    _ => {
                                        panic!("Got non string literal for field");
                                    }
                                }
                            } else if ident == "tag_key" {
                                match lit {
                                    Lit::Str(litstr) => {
                                        tag_key = litstr.value();
                                    }
                                    _ => {
                                        panic!("Got non string literal for tag_key");
                                    }
                                }
                            } else {
                                panic!("Unknown attribute")
                            }
                        }
                        _ => panic!("Unsupported attribute"),
                    },
                }
            }
        }
    }

    let type_name = &s.ast().ident;
    let tag_key_str = LitStr::new(&tag_key, Span::call_site());
    let mut from_value_body = TokenStream::new();
    let mut to_value_body = TokenStream::new();
    let mut process_value_body = TokenStream::new();
    let mut serialize_body = TokenStream::new();
    let mut extract_child_meta_body = TokenStream::new();

    let process_state_clone = if process_func.is_some() {
        Some(quote! {
            let __state_clone = __state.clone();
        })
    } else {
        None
    };
    let invoke_process_func = process_func.map(|func_name| {
        let func_name = Ident::new(&func_name, Span::call_site());
        quote! {
            let __result = __processor.#func_name(__result, __state_clone);
        }
    });

    for variant in s.variants() {
        let mut variant_name = &variant.ast().ident;
        let mut tag = Some(variant.ast().ident.to_string().to_lowercase());

        for attr in variant.ast().attrs {
            let meta = match attr.interpret_meta() {
                Some(meta) => meta,
                None => continue,
            };
            if meta.name() != "metastructure" {
                continue;
            }

            if let Meta::List(metalist) = meta {
                for nested_meta in metalist.nested {
                    match nested_meta {
                        NestedMeta::Literal(..) => panic!("unexpected literal attribute"),
                        NestedMeta::Meta(meta) => match meta {
                            Meta::Word(ident) => {
                                if ident == "fallback_variant" {
                                    tag = None;
                                } else {
                                    panic!("Unknown attribute {}", ident);
                                }
                            }
                            Meta::NameValue(MetaNameValue { ident, lit, .. }) => {
                                if ident == "tag" {
                                    match lit {
                                        Lit::Str(litstr) => {
                                            tag = Some(litstr.value());
                                        }
                                        _ => {
                                            panic!("Got non string literal for tag");
                                        }
                                    }
                                } else {
                                    panic!("Unknown key {}", ident);
                                }
                            }
                            other => {
                                panic!("Unexpected or bad attribute {}", other.name());
                            }
                        },
                    }
                }
            }
        }

        if let Some(tag) = tag {
            let tag = LitStr::new(&tag, Span::call_site());
            (quote! {
                Some(#tag) => {
                    __processor::FromValue::from_value(__meta::Annotated(Some(__meta::Value::Object(__object)), __meta))
                        .map_value(|__value| #type_name::#variant_name(Box::new(__value)))
                }
            }).to_tokens(&mut from_value_body);
            (quote! {
                __meta::Annotated(Some(#type_name::#variant_name(__value)), __meta) => {
                    let mut __rv = __processor::ToValue::to_value(__meta::Annotated(Some(__value), __meta));
                    if let __meta::Annotated(Some(__meta::Value::Object(ref mut __object)), _) = __rv {
                        __object.insert(#tag_key_str.to_string(), Annotated::new(__meta::Value::String(#tag.to_string())));
                    }
                    __rv
                }
            }).to_tokens(&mut to_value_body);
            (quote! {
                #type_name::#variant_name(ref __value) => {
                    __processor::ToValue::extract_child_meta(__value)
                }
            }).to_tokens(&mut extract_child_meta_body);
            (quote! {
                #type_name::#variant_name(ref __value) => {
                    let mut __map_ser = __serde::Serializer::serialize_map(__serializer, None)?;
                    __processor::ToValue::serialize_payload(__value, __serde::private::ser::FlatMapSerializer(&mut __map_ser))?;
                    __serde::ser::SerializeMap::serialize_key(&mut __map_ser, #tag_key_str)?;
                    __serde::ser::SerializeMap::serialize_value(&mut __map_ser, #tag)?;
                    __serde::ser::SerializeMap::end(__map_ser)
                }
            }).to_tokens(&mut serialize_body);
            (quote! {
                __meta::Annotated(Some(#type_name::#variant_name(__value)), __meta) => {
                    __processor::ProcessValue::process_value(__meta::Annotated(Some(*__value), __meta), __processor, __state)
                        .map_value(|__value| #type_name::#variant_name(Box::new(__value)))
                }
            }).to_tokens(&mut process_value_body);
        } else {
            (quote! {
                _ => {
                    if let Some(__type) = __type {
                        __object.insert(#tag_key_str.to_string(), __type);
                    }
                    __meta::Annotated(Some(#type_name::#variant_name(__object)), __meta)
                }
            }).to_tokens(&mut from_value_body);
            (quote! {
                __meta::Annotated(Some(#type_name::#variant_name(__value)), __meta) => {
                    __processor::ToValue::to_value(__meta::Annotated(Some(__value), __meta))
                }
            }).to_tokens(&mut to_value_body);
            (quote! {
                #type_name::#variant_name(ref __value) => {
                    __processor::ToValue::extract_child_meta(__value)
                }
            }).to_tokens(&mut extract_child_meta_body);
            (quote! {
                #type_name::#variant_name(ref __value) => {
                    __processor::ToValue::serialize_payload(__value, __serializer)
                }
            }).to_tokens(&mut serialize_body);
            (quote! {
                __meta::Annotated(Some(#type_name::#variant_name(__value)), __meta) => {
                    __processor::ProcessValue::process_value(__meta::Annotated(Some(__value), __meta), __processor, __state)
                        .map_value(#type_name::#variant_name)
                }
            }).to_tokens(&mut process_value_body);
        }
    }

    Ok(match t {
        Trait::FromValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;
                use types as __types;

                gen impl __processor::FromValue for @Self {
                    fn from_value(
                        __value: __meta::Annotated<__meta::Value>,
                    ) -> __meta::Annotated<Self> {
                        match __types::Object::<__meta::Value>::from_value(__value) {
                            __meta::Annotated(Some(mut __object), __meta) => {
                                let __type = __object.remove(#tag_key_str);
                                match __type.as_ref().and_then(|__type| __type.0.as_ref()).and_then(|__type| __type.as_str()) {
                                    #from_value_body
                                }
                            }
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta)
                        }
                    }
                }
            })
        }
        Trait::ToValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use types as __types;
                use meta as __meta;
                extern crate serde as __serde;

                gen impl __processor::ToValue for @Self {
                    fn to_value(
                        __value: __meta::Annotated<Self>
                    ) -> __meta::Annotated<__meta::Value> {
                        match __value {
                            #to_value_body
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta),
                        }
                    }

                    fn serialize_payload<S>(&self, __serializer: S) -> Result<S::Ok, S::Error>
                    where
                        S: __serde::ser::Serializer
                    {
                        match *self {
                            #serialize_body
                        }
                    }

                    fn extract_child_meta(&self) -> __meta::MetaMap
                    where
                        Self: Sized,
                    {
                        match *self {
                            #extract_child_meta_body
                        }
                    }
                }
            })
        }
        Trait::ProcessValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;

                gen impl __processor::ProcessValue for @Self {
                    fn process_value<P: __processor::Processor>(
                        __value: __meta::Annotated<Self>,
                        __processor: &P,
                        __state: __processor::ProcessingState
                    ) -> __meta::Annotated<Self> {
                        #process_state_clone
                        let __result = match __value {
                            #process_value_body
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta),
                        };
                        #invoke_process_func
                        __result
                    }
                }
            })
        }
    })
}

fn process_metastructure_impl(s: synstructure::Structure, t: Trait) -> TokenStream {
    let s = match process_wrapper_struct_derive(s, t) {
        Ok(stream) => return stream,
        Err(s) => s,
    };

    let mut s = match process_enum_struct_derive(s, t) {
        Ok(stream) => return stream,
        Err(s) => s,
    };

    s.add_bounds(synstructure::AddBounds::Generics);

    let variants = s.variants();
    if variants.len() != 1 {
        panic!("Can only derive structs");
    }

    let mut variant = variants[0].clone();
    for binding in variant.bindings_mut() {
        binding.style = synstructure::BindStyle::MoveMut;
    }
    let mut from_value_body = TokenStream::new();
    let mut to_value_body = TokenStream::new();
    let mut process_value_body = TokenStream::new();
    let mut serialize_body = TokenStream::new();
    let mut extract_child_meta_body = TokenStream::new();
    let mut process_func = None;
    let mut tmp_idx = 0;

    for attr in &s.ast().attrs {
        let meta = match attr.interpret_meta() {
            Some(meta) => meta,
            None => continue,
        };
        if meta.name() != "metastructure" {
            continue;
        }

        if let Meta::List(metalist) = meta {
            for nested_meta in metalist.nested {
                match nested_meta {
                    NestedMeta::Literal(..) => panic!("unexpected literal attribute"),
                    NestedMeta::Meta(meta) => match meta {
                        Meta::NameValue(MetaNameValue { ident, lit, .. }) => {
                            if ident == "process_func" {
                                match lit {
                                    Lit::Str(litstr) => {
                                        process_func = Some(litstr.value());
                                    }
                                    _ => {
                                        panic!("Got non string literal for field");
                                    }
                                }
                            } else {
                                panic!("Unknown attribute")
                            }
                        }
                        _ => panic!("Unsupported attribute"),
                    },
                }
            }
        }
    }

    for bi in variant.bindings() {
        let mut additional_properties = false;
        let mut field_name = bi
            .ast()
            .ident
            .as_ref()
            .expect("can not derive struct tuples")
            .to_string();
        let mut cap_size_attr = quote!(None);
        let mut pii_kind_attr = quote!(None);
        let mut required = false;
        let mut legacy_aliases = vec![];
        for attr in &bi.ast().attrs {
            let meta = match attr.interpret_meta() {
                Some(meta) => meta,
                None => continue,
            };
            if meta.name() != "metastructure" {
                continue;
            }

            if let Meta::List(metalist) = meta {
                for nested_meta in metalist.nested {
                    match nested_meta {
                        NestedMeta::Literal(..) => panic!("unexpected literal attribute"),
                        NestedMeta::Meta(meta) => match meta {
                            Meta::Word(ident) => {
                                if ident == "additional_properties" {
                                    additional_properties = true;
                                } else {
                                    panic!("Unknown attribute {}", ident);
                                }
                            }
                            Meta::NameValue(MetaNameValue { ident, lit, .. }) => {
                                if ident == "field" {
                                    match lit {
                                        Lit::Str(litstr) => {
                                            field_name = litstr.value();
                                        }
                                        _ => {
                                            panic!("Got non string literal for field");
                                        }
                                    }
                                } else if ident == "required" {
                                    match lit {
                                        Lit::Str(litstr) => match litstr.value().as_str() {
                                            "true" => required = true,
                                            "false" => required = false,
                                            other => panic!("Unknown value {}", other),
                                        },
                                        _ => {
                                            panic!("Got non string literal for required");
                                        }
                                    }
                                } else if ident == "cap_size" {
                                    match lit {
                                        Lit::Str(litstr) => {
                                            let attr = parse_cap_size(litstr.value().as_str());
                                            cap_size_attr = quote!(Some(#attr));
                                        }
                                        _ => {
                                            panic!("Got non string literal for cap_size");
                                        }
                                    }
                                } else if ident == "pii_kind" {
                                    match lit {
                                        Lit::Str(litstr) => {
                                            let attr = parse_pii_kind(litstr.value().as_str());
                                            pii_kind_attr = quote!(Some(#attr));
                                        }
                                        _ => {
                                            panic!("Got non string literal for pii_kind");
                                        }
                                    }
                                } else if ident == "legacy_alias" {
                                    match lit {
                                        Lit::Str(litstr) => {
                                            legacy_aliases.push(litstr.value());
                                        }
                                        _ => {
                                            panic!("Got non string literal for legacy_alias");
                                        }
                                    }
                                }
                            }
                            other => {
                                panic!("Unexpected or bad attribute {}", other.name());
                            }
                        },
                    }
                }
            }
        }

        let field_name = LitStr::new(&field_name, Span::call_site());

        if additional_properties {
            (quote! {
                let #bi = __obj.into_iter().map(|(__key, __value)| (__key, __processor::FromValue::from_value(__value))).collect();
            }).to_tokens(&mut from_value_body);
            (quote! {
                __map.extend(#bi.into_iter().map(|(__key, __value)| (__key, __processor::ToValue::to_value(__value))));
            }).to_tokens(&mut to_value_body);
            (quote! {
                let #bi = #bi.into_iter().map(|(__key, __value)| {
                    let __value = __processor::ProcessValue::process_value(__value, __processor, __state.enter_borrowed(__key.as_str(), None));
                    (__key, __value)
                }).collect();
            }).to_tokens(&mut process_value_body);
            (quote! {
                for (__key, __value) in #bi.iter() {
                    if !__value.skip_serialization() {
                        __serde::ser::SerializeMap::serialize_key(&mut __map_serializer, __key)?;
                        __serde::ser::SerializeMap::serialize_value(&mut __map_serializer, &__processor::SerializePayload(__value))?;
                    }
                }
            }).to_tokens(&mut serialize_body);
            (quote! {
                for (__key, __value) in #bi.iter() {
                    let __inner_tree = __processor::ToValue::extract_meta_tree(__value);
                    if !__inner_tree.is_empty() {
                        __child_meta.insert(__key.to_string(), __inner_tree);
                    }
                }
            }).to_tokens(&mut extract_child_meta_body);
        } else {
            let field_attrs_name = Ident::new(
                &format!("__field_attrs_{}", {
                    tmp_idx += 1;
                    tmp_idx
                }),
                Span::call_site(),
            );
            let required_attr = LitBool {
                value: required,
                span: Span::call_site(),
            };

            (quote! {
                let #bi = __obj.remove(#field_name);
            }).to_tokens(&mut from_value_body);

            for legacy_alias in legacy_aliases {
                let legacy_field_name = LitStr::new(&legacy_alias, Span::call_site());
                (quote! {
                    let #bi = #bi.or(__obj.remove(#legacy_field_name));
                }).to_tokens(&mut from_value_body);
            }

            (quote! {
                let #bi = __processor::FromValue::from_value(#bi.unwrap_or_else(|| __meta::Annotated(None, __meta::Meta::default())));
            }).to_tokens(&mut from_value_body);

            if required {
                (quote! {
                    let #bi = #bi.require_value();
                }).to_tokens(&mut from_value_body);
            }
            (quote! {
                __map.insert(#field_name.to_string(), __processor::ToValue::to_value(#bi));
            }).to_tokens(&mut to_value_body);
            (quote! {
                const #field_attrs_name: __processor::FieldAttrs = __processor::FieldAttrs {
                    name: Some(#field_name),
                    required: #required_attr,
                    cap_size: #cap_size_attr,
                    pii_kind: #pii_kind_attr,
                };
                let #bi = __processor::ProcessValue::process_value(#bi, __processor, __state.enter_static(#field_name, Some(::std::borrow::Cow::Borrowed(&#field_attrs_name))));
            }).to_tokens(&mut process_value_body);
            (quote! {
                if !#bi.skip_serialization() {
                    __serde::ser::SerializeMap::serialize_key(&mut __map_serializer, #field_name)?;
                    __serde::ser::SerializeMap::serialize_value(&mut __map_serializer, &__processor::SerializePayload(#bi))?;
                }
            }).to_tokens(&mut serialize_body);
            (quote! {
                let __inner_tree = __processor::ToValue::extract_meta_tree(#bi);
                if !__inner_tree.is_empty() {
                    __child_meta.insert(#field_name.to_string(), __inner_tree);
                }
            }).to_tokens(&mut extract_child_meta_body);
        }
    }

    let ast = s.ast();
    let expectation = LitStr::new(
        &format!("expected {}", ast.ident.to_string().to_lowercase()),
        Span::call_site(),
    );
    let mut variant = variant.clone();
    for binding in variant.bindings_mut() {
        binding.style = synstructure::BindStyle::Move;
    }
    let to_value_pat = variant.pat();
    let to_structure_assemble_pat = variant.pat();
    for binding in variant.bindings_mut() {
        binding.style = synstructure::BindStyle::Ref;
    }
    let serialize_pat = variant.pat();

    let invoke_process_func = process_func.map(|func_name| {
        let func_name = Ident::new(&func_name, Span::call_site());
        quote! {
            let __result = __processor.#func_name(__result, __state);
        }
    });

    match t {
        Trait::FromValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;

                gen impl __processor::FromValue for @Self {
                    fn from_value(
                        __value: __meta::Annotated<__meta::Value>,
                    ) -> __meta::Annotated<Self> {
                        match __value {
                            __meta::Annotated(Some(__meta::Value::Object(mut __obj)), __meta) => {
                                #from_value_body;
                                __meta::Annotated(Some(#to_structure_assemble_pat), __meta)
                            }
                            __meta::Annotated(None, __meta) => __meta::Annotated(None, __meta),
                            __meta::Annotated(Some(__value), mut __meta) => {
                                __meta.add_unexpected_value_error(#expectation, __value);
                                __meta::Annotated(None, __meta)
                            }
                        }
                    }
                }
            })
        }
        Trait::ToValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use types as __types;
                use meta as __meta;
                extern crate serde as __serde;

                gen impl __processor::ToValue for @Self {
                    fn to_value(
                        __value: __meta::Annotated<Self>
                    ) -> __meta::Annotated<__meta::Value> {
                        let __meta::Annotated(__value, __meta) = __value;
                        if let Some(__value) = __value {
                            let mut __map = __types::Object::new();
                            let #to_value_pat = __value;
                            #to_value_body;
                            __meta::Annotated(Some(__meta::Value::Object(__map)), __meta)
                        } else {
                            __meta::Annotated(None, __meta)
                        }
                    }

                    fn serialize_payload<S>(&self, __serializer: S) -> Result<S::Ok, S::Error>
                    where
                        Self: Sized,
                        S: __serde::ser::Serializer
                    {
                        let #serialize_pat = *self;
                        let mut __map_serializer = __serde::ser::Serializer::serialize_map(__serializer, None)?;
                        #serialize_body;
                        __serde::ser::SerializeMap::end(__map_serializer)
                    }

                    fn extract_child_meta(&self) -> __meta::MetaMap
                    where
                        Self: Sized,
                    {
                        let mut __child_meta = __meta::MetaMap::new();
                        let #serialize_pat = *self;
                        #extract_child_meta_body;
                        __child_meta
                    }
                }
            })
        }
        Trait::ProcessValue => {
            s.gen_impl(quote! {
                use processor as __processor;
                use meta as __meta;

                gen impl __processor::ProcessValue for @Self {
                    fn process_value<P: __processor::Processor>(
                        __value: __meta::Annotated<Self>,
                        __processor: &P,
                        __state: __processor::ProcessingState
                    ) -> __meta::Annotated<Self> {
                        let __meta::Annotated(__value, __meta) = __value;
                        let __result = if let Some(__value) = __value {
                            let #to_value_pat = __value;
                            #process_value_body;
                            __meta::Annotated(Some(#to_structure_assemble_pat), __meta)
                        } else {
                            __meta::Annotated(None, __meta)
                        };
                        #invoke_process_func
                        __result
                    }
                }
            })
        }
    }
}

fn parse_cap_size(name: &str) -> TokenStream {
    match name {
        "enumlike" => quote!(__processor::CapSize::EnumLike),
        "summary" => quote!(__processor::CapSize::Summary),
        "message" => quote!(__processor::CapSize::Message),
        "symbol" => quote!(__processor::CapSize::Symbol),
        "path" => quote!(__processor::CapSize::Path),
        "short_path" => quote!(__processor::CapSize::ShortPath),
        _ => panic!("invalid cap_size variant '{}'", name),
    }
}

fn parse_pii_kind(kind: &str) -> TokenStream {
    match kind {
        "freeform" => quote!(__processor::PiiKind::Freeform),
        "ip" => quote!(__processor::PiiKind::Ip),
        "id" => quote!(__processor::PiiKind::Id),
        "username" => quote!(__processor::PiiKind::Username),
        "hostname" => quote!(__processor::PiiKind::Hostname),
        "sensitive" => quote!(__processor::PiiKind::Sensitive),
        "name" => quote!(__processor::PiiKind::Name),
        "email" => quote!(__processor::PiiKind::Email),
        "location" => quote!(__processor::PiiKind::Location),
        "databag" => quote!(__processor::PiiKind::Databag),
        _ => panic!("invalid pii_kind variant '{}'", kind),
    }
}
