use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, Member, Meta, Type};

const NO_MODULES: &str = "a node needs at least one `#[module]` field; a leaf module is a plain struct with a `#[listeners]` impl block and no derive";
const NOT_A_STRUCT: &str = "a node is a struct whose `#[module]` fields are its modules";
const MODULE_TAKES_NOTHING: &str = "`#[module]` on a field takes no arguments";
const BAD_OWN: &str = "`#[module(own)]` is the only argument a node takes: it declares that the struct also has its own `#[listeners] impl` block";

pub(crate) struct Field<'a> {
    member: Member,
    label: String,
    ty: &'a Type,
}

pub(crate) fn fields(input: &DeriveInput) -> Result<Vec<Field<'_>>, Error> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(&input.ident, NOT_A_STRUCT));
    };
    let list: Vec<&syn::Field> = match &data.fields {
        Fields::Named(named) => named.named.iter().collect(),
        Fields::Unnamed(unnamed) => unnamed.unnamed.iter().collect(),
        Fields::Unit => Vec::new(),
    };
    let mut found = Vec::new();
    for (index, field) in list.into_iter().enumerate() {
        let mut marked = false;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("module"))
        {
            if !matches!(attr.meta, Meta::Path(_)) {
                return Err(Error::new_spanned(attr, MODULE_TAKES_NOTHING));
            }
            marked = true;
        }
        if marked {
            let member = field.ident.as_ref().map_or_else(
                || Member::Unnamed(index.into()),
                |ident| Member::Named(ident.clone()),
            );
            let label = match &member {
                Member::Named(ident) => ident.to_string(),
                Member::Unnamed(index) => index.index.to_string(),
            };
            found.push(Field {
                member,
                label,
                ty: &field.ty,
            });
        }
    }
    Ok(found)
}

pub(crate) fn own_attribute(input: &DeriveInput, name: &str) -> Result<bool, Error> {
    let mut own = false;
    for attr in input.attrs.iter().filter(|attr| attr.path().is_ident(name)) {
        let Meta::List(_) = &attr.meta else {
            return Err(Error::new_spanned(attr, BAD_OWN));
        };
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("own") {
                own = true;
                Ok(())
            } else if name == "plugin" {
                meta.value()?.parse::<syn::Expr>().map(drop)
            } else {
                Err(meta.error(BAD_OWN))
            }
        })?;
    }
    Ok(own)
}

pub(crate) fn derive(input: &DeriveInput) -> Result<TokenStream, Error> {
    let own = own_attribute(input, "module")?;
    let fields = fields(input)?;
    if fields.is_empty() {
        return Err(Error::new_spanned(&input.ident, NO_MODULES));
    }
    Ok(expand(input, &fields, own))
}

pub(crate) fn expand(input: &DeriveInput, fields: &[Field<'_>], own: bool) -> TokenStream {
    let node = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let tys: Vec<&Type> = fields.iter().map(|field| field.ty).collect();
    let members: Vec<&Member> = fields.iter().map(|field| &field.member).collect();
    let labels: Vec<&String> = fields.iter().map(|field| &field.label).collect();
    let on = |ty: &Type| quote_spanned!(ty.span()=> <#ty as ::rspigot_plugin::Listeners>);
    let on: Vec<TokenStream> = tys.iter().map(|ty| on(ty)).collect();
    let handler_slots: Vec<TokenStream> = tys
        .iter()
        .zip(&on)
        .map(|(ty, on)| {
            quote_spanned!(ty.span()=> ::rspigot_plugin::compose::handler_slots(#on::HANDLER_SLOTS))
        })
        .collect();
    let command_slots: Vec<TokenStream> = tys
        .iter()
        .zip(&on)
        .map(|(ty, on)| {
            quote_spanned!(ty.span()=> ::rspigot_plugin::compose::command_slots(#on::COMMAND_SLOTS))
        })
        .collect();
    let push_handlers = tys
        .iter()
        .zip(&on)
        .zip(&handler_slots)
        .map(|((ty, on), slots)| {
            quote_spanned! {ty.span()=>
                ::rspigot_plugin::compose::push_handlers(out, at, #on::handlers(), #slots);
                let at = at + #slots;
            }
        });
    let push_commands = tys
        .iter()
        .zip(&on)
        .zip(&command_slots)
        .map(|((ty, on), slots)| {
            quote_spanned! {ty.span()=>
                ::rspigot_plugin::compose::push_commands(out, at, #on::commands());
                let at = at + #slots;
            }
        });
    let push_modules =
        tys.iter()
            .zip(&on)
            .zip(&command_slots)
            .zip(&labels)
            .map(|(((ty, on), slots), label)| {
                quote_spanned! {ty.span()=>
                    let mut inner = ::std::vec::Vec::new();
                    #on::command_modules(at, &mut inner);
                    ::rspigot_plugin::compose::push_command_modules(out, #label, inner);
                    let at = at + #slots;
                }
            });
    let dispatch_steps = tys
        .iter()
        .zip(&members)
        .zip(&handler_slots)
        .zip(&on)
        .map(|(((ty, member), slots), on)| {
            quote_spanned! {ty.span()=>
                if let ::core::option::Option::Some(local) = ::rspigot_plugin::compose::local_handler(
                    handler,
                    at,
                    #slots,
                    #on::TAB_SLOT,
                ) {
                    return #on::dispatch_at(&self.#member, local, call).await;
                }
                let at = at + #slots;
            }
        });
    let task_steps = tys.iter().zip(&members).zip(&on).map(
        |((ty, member), on)| quote_spanned!(ty.span()=> #on::run_tasks(&self.#member, server, clock).await;),
    );
    let invoke_steps =
        tys.iter()
            .zip(&members)
            .zip(&command_slots)
            .zip(&on)
            .map(|(((ty, member), slots), on)| {
                quote_spanned! {ty.span()=>
                    if ::rspigot_plugin::compose::owns_command(command, at, #slots) {
                        return #on::invoke(&self.#member, invocation.rebased(at)).await;
                    }
                    let at = at + #slots;
                }
            });
    let broadcast_steps = tys.iter().zip(&members).zip(&on).map(|((ty, member), on)| {
        quote_spanned! {ty.span()=>
            #on::invoke(&self.#member, invocation.clone()).await;
        }
    });
    let enables = tys.iter().zip(&members).zip(&on).map(
        |((ty, member), on)| quote_spanned!(ty.span()=> #on::enable(&self.#member, server).await;),
    );
    let disables =
        tys.iter().zip(&members).zip(&on).rev().map(
            |((ty, member), on)| quote_spanned!(ty.span()=> #on::disable(&self.#member).await;),
        );
    let children = quote!(::rspigot_plugin::compose::Children);
    let assert_own = (own && input.generics.params.is_empty()).then(|| {
        let span = node.span();
        let ty = quote_spanned!(span=> #node);
        quote! {
            const _: () = {
                const fn assert_listeners<M: ::rspigot_plugin::Listeners>() {}
                assert_listeners::<#ty>();
            };
        }
    });
    let forwarding = (!own).then(|| {
        quote! {
            #[automatically_derived]
            impl #impl_generics ::rspigot_plugin::Listeners for #node #type_generics #where_clause {
                const HANDLER_SLOTS: u32 = <Self as #children>::HANDLER_SLOTS;
                const COMMAND_SLOTS: u32 = <Self as #children>::COMMAND_SLOTS;

                fn handlers() -> ::std::vec::Vec<::rspigot_plugin::HandlerSpec> {
                    let mut out = ::std::vec::Vec::new();
                    <Self as #children>::handlers(0, &mut out);
                    out
                }

                fn commands() -> ::std::vec::Vec<::rspigot_plugin::CommandSpec> {
                    let mut out = ::std::vec::Vec::new();
                    <Self as #children>::commands(0, &mut out);
                    out
                }

                fn command_modules(
                    base: u32,
                    out: &mut ::std::vec::Vec<::rspigot_plugin::CommandModule>,
                ) {
                    <Self as #children>::command_modules(base, out);
                }

                async fn dispatch_at(
                    &self,
                    handler: u32,
                    call: ::rspigot_plugin::Call,
                ) -> ::rspigot_plugin::Outcome {
                    <Self as #children>::dispatch(self, 0, handler, call).await
                }

                async fn enable(&self, server: &::rspigot_plugin::Server) {
                    <Self as #children>::enable(self, server).await;
                }

                async fn disable(&self) {
                    <Self as #children>::disable(self).await;
                }

                async fn run_tasks(
                    &self,
                    server: &::rspigot_plugin::Server,
                    clock: &mut ::rspigot_plugin::TaskClock,
                ) {
                    <Self as #children>::run_tasks(self, server, clock).await;
                }

                async fn invoke(&self, invocation: ::rspigot_plugin::Invocation) {
                    <Self as #children>::invoke(self, 0, invocation).await;
                }
            }
        }
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #children for #node #type_generics #where_clause {
            const HANDLER_SLOTS: u32 = 0 #(+ #handler_slots)*;
            const COMMAND_SLOTS: u32 = 0 #(+ #command_slots)*;

            fn handlers(
                base: u32,
                out: &mut ::std::vec::Vec<::rspigot_plugin::HandlerSpec>,
            ) {
                let at = base;
                #(#push_handlers)*
                let _ = at;
            }

            fn commands(
                base: u32,
                out: &mut ::std::vec::Vec<::rspigot_plugin::CommandSpec>,
            ) {
                let at = base;
                #(#push_commands)*
                let _ = at;
            }

            fn command_modules(
                base: u32,
                out: &mut ::std::vec::Vec<::rspigot_plugin::CommandModule>,
            ) {
                let at = base;
                #(#push_modules)*
                let _ = at;
            }

            async fn dispatch(
                &self,
                base: u32,
                handler: u32,
                call: ::rspigot_plugin::Call,
            ) -> ::rspigot_plugin::Outcome {
                let at = base;
                #(#dispatch_steps)*
                let _ = (at, handler, call);
                ::rspigot_plugin::Outcome::Observed
            }

            async fn enable(&self, server: &::rspigot_plugin::Server) {
                #(#enables)*
            }

            async fn disable(&self) {
                #(#disables)*
            }

            async fn run_tasks(
                &self,
                server: &::rspigot_plugin::Server,
                clock: &mut ::rspigot_plugin::TaskClock,
            ) {
                #(#task_steps)*
            }

            async fn invoke(&self, base: u32, invocation: ::rspigot_plugin::Invocation) {
                let command = invocation.command;
                if command > base + <Self as #children>::COMMAND_SLOTS {
                    #(#broadcast_steps)*
                    return;
                }
                let at = base;
                #(#invoke_steps)*
                let _ = (at, invocation);
            }
        }

        #[automatically_derived]
        impl #impl_generics #node #type_generics #where_clause {
            #[doc(hidden)]
            pub const __RSPIGOT_KIDS: ::rspigot_plugin::compose::Present =
                ::rspigot_plugin::compose::Present;
        }

        #forwarding

        #assert_own
    }
}
