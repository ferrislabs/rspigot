use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Error, FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, Meta, Receiver, ReturnType,
    Signature, Type,
};

use crate::commands::{self, Command};
use crate::events::EVENTS;

const NOT_A_HANDLER: &str = "a #[listeners] block only holds handlers: `async fn name(&self, event: &Event)`; put helpers in another impl block";
const MUT_SELF: &str = "handlers take `&self`; keep state in fields behind a `Mutex` or atomics";
const NOT_AN_EVENT: &str = "a handler takes an rspigot_plugin event as `&Event` to observe it (for example `&PlayerJoin`), or as `&mut Event` to decide on it; the events are listed in rspigot-plugin's EVENTS.md";
const BAD_ON_ENABLE: &str = "`on_enable` takes `&self, server: &Server`";
const BAD_ON_DISABLE: &str = "`on_disable` takes `&self`";
const NOT_DECIDABLE: &str = "`&mut` is only allowed for events a handler can decide (cancel or change); this event can only be observed with `&`";
const IGNORE_CANCELLED_ON_DECIDER: &str =
    "`#[ignore_cancelled]` only applies to observing handlers (`&Event`)";
const NOT_A_PRIORITY: &str = "`#[priority(...)]` takes one of Lowest, Low, Normal, High, Highest";
const PRIORITIES: [&str; 5] = ["Lowest", "Low", "Normal", "High", "Highest"];

struct Handler {
    cfgs: Vec<Attribute>,
    id: u32,
    method: Ident,
    kind: Ident,
    decides: bool,
    priority: Ident,
    ignore_cancelled: bool,
}

#[derive(Default)]
struct Hooks {
    enable: Vec<Vec<Attribute>>,
    disable: Vec<Vec<Attribute>>,
}

#[derive(Default)]
struct Marks {
    priority: Option<Ident>,
    ignore_cancelled: Option<Attribute>,
}

pub(crate) fn expand(args: TokenStream, mut block: ItemImpl) -> TokenStream {
    let mut errors = Vec::new();
    if !args.is_empty() {
        errors.push(Error::new_spanned(&args, "#[listeners] takes no arguments"));
    }
    let mut hooks = Hooks::default();
    let mut dynamic = false;
    let mut declared: Vec<Command> = Vec::new();
    let handlers: Vec<Handler> = if let Some((_, path, _)) = &block.trait_ {
        errors.push(Error::new_spanned(
            path,
            "#[listeners] goes on the plugin's own impl block (`impl MyPlugin { ... }`), not on a trait impl",
        ));
        for item in &mut block.items {
            if let ImplItem::Fn(method) = item {
                method.attrs.retain(|attr| !is_mark(attr));
            }
        }
        Vec::new()
    } else {
        block
            .items
            .iter_mut()
            .zip(1..)
            .filter_map(|(item, id)| match item {
                ImplItem::Fn(method) if method.sig.ident == "on_enable" => {
                    hook(method, true, &mut hooks.enable, &mut errors);
                    None
                }
                ImplItem::Fn(method) if method.sig.ident == "on_disable" => {
                    hook(method, false, &mut hooks.disable, &mut errors);
                    None
                }
                ImplItem::Fn(method) if method.sig.ident == "on_command" => {
                    dynamic = true;
                    None
                }
                ImplItem::Fn(method) if commands::marked(method) => {
                    let cfgs = cfgs(&method.attrs);
                    declared.extend(commands::read(method, id, cfgs, &mut errors));
                    None
                }
                ImplItem::Fn(method) => handler(method, id, &mut errors),
                other => {
                    errors.push(Error::new_spanned(other, NOT_A_HANDLER));
                    None
                }
            })
            .collect()
    };
    let suggesting = commands::suggests(&declared);
    let own_ids = handlers.iter().map(|handler| handler.id).max().unwrap_or(0);
    let own_commands = declared.iter().map(|command| command.id).max().unwrap_or(0);
    let own_ids = Literal::u32_unsuffixed(own_ids);
    let own_commands = Literal::u32_unsuffixed(own_commands);
    let kids = quote!(&Self::__RSPIGOT_KIDS);
    let present = quote!(::rspigot_plugin::compose::present::<Self, _>(#kids));
    let own_slots = if suggesting {
        quote!(#own_ids + 1)
    } else {
        quote!(#own_ids)
    };
    let tab = quote! {
        (if #present { #own_ids + 1 } else { ::core::primitive::u32::MAX })
    };
    let tab_slot = if suggesting {
        quote!(!#present)
    } else {
        quote!(false)
    };
    let specs = handlers
        .iter()
        .map(spec)
        .chain(suggesting.then(|| commands::suggestion_spec(&tab)));
    let arms = handlers
        .iter()
        .map(arm)
        .chain(suggesting.then(|| commands::suggestion_arm(&declared, &tab)));
    let command_specs = commands::specs(&declared);
    let command_match = if declared.is_empty() && !dynamic {
        quote!(let _ = command;)
    } else {
        commands::invoke_match(&declared, dynamic)
    };
    let enables = hooks.enable.iter().map(|cfgs| {
        quote! {
            #(#cfgs)*
            self.on_enable(server).await;
        }
    });
    let disables = hooks.disable.iter().map(|cfgs| {
        quote! {
            #(#cfgs)*
            self.on_disable().await;
        }
    });
    let errors = errors.iter().map(Error::to_compile_error);
    let plugin = &block.self_ty;
    let (impl_generics, _, where_clause) = block.generics.split_for_impl();
    let kids_trait = quote!(<_ as ::rspigot_plugin::compose::Kids<Self>>);
    quote! {
        #block

        #[allow(unused_imports)]
        const _: () = {
            use ::rspigot_plugin::compose::Detect as _;

            impl #impl_generics ::rspigot_plugin::Listeners for #plugin #where_clause {
                const HANDLER_SLOTS: u32 =
                    #own_slots
                        + ::rspigot_plugin::compose::kids_handler_slots::<Self, _>(#kids);

                const COMMAND_SLOTS: u32 =
                    #own_commands
                        + ::rspigot_plugin::compose::kids_command_slots::<Self, _>(#kids);

                const TAB_SLOT: bool = #tab_slot;

                fn handlers() -> ::std::vec::Vec<::rspigot_plugin::HandlerSpec> {
                    let mut out = ::std::vec![#(#specs),*];
                    #kids_trait::handlers(#kids, #own_slots, &mut out);
                    out
                }

                async fn dispatch_at(
                    &self,
                    handler: u32,
                    call: ::rspigot_plugin::Call,
                ) -> ::rspigot_plugin::Outcome {
                    if #present && handler > #own_slots {
                        return #kids_trait::dispatch(#kids, self, #own_slots, handler, call).await;
                    }
                    match (handler, call) {
                        #(#arms)*
                        _ => ::rspigot_plugin::Outcome::Observed,
                    }
                }

                async fn enable(&self, server: &::rspigot_plugin::Server) {
                    let _ = server;
                    #(#enables)*
                    #kids_trait::enable(#kids, self, server).await;
                }

                async fn disable(&self) {
                    #kids_trait::disable(#kids, self).await;
                    #(#disables)*
                }

                fn commands() -> ::std::vec::Vec<::rspigot_plugin::CommandSpec> {
                    let mut out = ::std::vec![#(#command_specs),*];
                    #kids_trait::commands(#kids, #own_commands, &mut out);
                    out
                }

                fn command_modules(
                    base: u32,
                    out: &mut ::std::vec::Vec<::rspigot_plugin::CommandModule>,
                ) {
                    ::rspigot_plugin::compose::push_own_module::<Self>(out, base, #own_commands);
                    #kids_trait::command_modules(#kids, base + #own_commands, out);
                }

                async fn invoke(&self, invocation: ::rspigot_plugin::Invocation) {
                    let command = invocation.command();
                    if #present
                        && command > #own_commands
                        && command <= #own_commands
                            + ::rspigot_plugin::compose::kids_command_slots::<Self, _>(#kids)
                    {
                        return #kids_trait::invoke(#kids, self, #own_commands, invocation).await;
                    }
                    let beyond = if #present
                        && command > #own_commands
                            + ::rspigot_plugin::compose::kids_command_slots::<Self, _>(#kids)
                    {
                        ::core::option::Option::Some(invocation.clone())
                    } else {
                        ::core::option::Option::None
                    };
                    #command_match
                    if let ::core::option::Option::Some(rest) = beyond {
                        #kids_trait::invoke(#kids, self, #own_commands, rest).await;
                    }
                }
            }
        };

        #(#errors)*
    }
}

fn handler(method: &mut ImplItemFn, id: u32, errors: &mut Vec<Error>) -> Option<Handler> {
    let marks = take_marks(&mut method.attrs, errors);
    let (kind, decides) = match event(&method.sig) {
        Ok(found) => found,
        Err(error) => {
            errors.push(error);
            return None;
        }
    };
    if decides && let Some(attr) = &marks.ignore_cancelled {
        errors.push(Error::new_spanned(attr, IGNORE_CANCELLED_ON_DECIDER));
        return None;
    }
    Some(Handler {
        cfgs: cfgs(&method.attrs),
        id,
        method: method.sig.ident.clone(),
        kind,
        decides,
        priority: marks
            .priority
            .unwrap_or_else(|| Ident::new("Normal", Span::call_site())),
        ignore_cancelled: marks.ignore_cancelled.is_some(),
    })
}

fn hook(
    method: &ImplItemFn,
    takes_server: bool,
    hooks: &mut Vec<Vec<Attribute>>,
    errors: &mut Vec<Error>,
) {
    let mut inputs = method.sig.inputs.iter();
    let by_ref = matches!(
        inputs.next(),
        Some(FnArg::Receiver(Receiver {
            reference: Some(_),
            mutability: None,
            ..
        }))
    );
    let server = !takes_server
        || matches!(
            inputs.next(),
            Some(FnArg::Typed(argument))
                if matches!(referenced(&argument.ty), Some((name, false)) if name == "Server")
        );
    if plain(&method.sig) && by_ref && server && inputs.next().is_none() {
        hooks.push(cfgs(&method.attrs));
    } else {
        let message = if takes_server {
            BAD_ON_ENABLE
        } else {
            BAD_ON_DISABLE
        };
        errors.push(Error::new_spanned(&method.sig, message));
    }
}

fn cfgs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .cloned()
        .collect()
}

fn is_mark(attr: &Attribute) -> bool {
    attr.path().is_ident("priority")
        || attr.path().is_ident("ignore_cancelled")
        || attr.path().is_ident("command")
}

fn take_marks(attrs: &mut Vec<Attribute>, errors: &mut Vec<Error>) -> Marks {
    let mut marks = Marks::default();
    for attr in std::mem::take(attrs) {
        if attr.path().is_ident("priority") {
            match priority(&attr) {
                Ok(_) if marks.priority.is_some() => errors.push(Error::new_spanned(
                    &attr,
                    "this handler already has a `#[priority(...)]`",
                )),
                Ok(level) => marks.priority = Some(level),
                Err(error) => errors.push(error),
            }
        } else if attr.path().is_ident("ignore_cancelled") {
            if !matches!(attr.meta, Meta::Path(_)) {
                errors.push(Error::new_spanned(
                    &attr,
                    "`#[ignore_cancelled]` takes no arguments",
                ));
            } else if marks.ignore_cancelled.is_some() {
                errors.push(Error::new_spanned(
                    &attr,
                    "this handler already has an `#[ignore_cancelled]`",
                ));
            } else {
                marks.ignore_cancelled = Some(attr);
            }
        } else {
            attrs.push(attr);
        }
    }
    marks
}

fn priority(attr: &Attribute) -> Result<Ident, Error> {
    let level = match &attr.meta {
        Meta::List(list) => syn::parse2::<Ident>(list.tokens.clone()).ok(),
        _ => None,
    };
    level
        .filter(|level| PRIORITIES.iter().any(|name| level == name))
        .ok_or_else(|| Error::new_spanned(attr, NOT_A_PRIORITY))
}

fn plain(sig: &Signature) -> bool {
    sig.asyncness.is_some()
        && sig.constness.is_none()
        && sig.unsafety.is_none()
        && sig.abi.is_none()
        && sig.generics.params.is_empty()
        && sig.generics.where_clause.is_none()
        && sig.variadic.is_none()
        && matches!(sig.output, ReturnType::Default)
}

fn event(sig: &Signature) -> Result<(Ident, bool), Error> {
    let plain = plain(sig);
    if plain
        && let Some(FnArg::Receiver(receiver)) = sig.inputs.first()
        && matches!(&*receiver.ty, Type::Reference(reference) if reference.mutability.is_some())
    {
        return Err(Error::new_spanned(receiver, MUT_SELF));
    }
    let mut inputs = sig.inputs.iter();
    let (
        true,
        Some(FnArg::Receiver(Receiver {
            reference: Some(_),
            mutability: None,
            ..
        })),
        Some(FnArg::Typed(argument)),
        None,
    ) = (plain, inputs.next(), inputs.next(), inputs.next())
    else {
        return Err(Error::new_spanned(sig, NOT_A_HANDLER));
    };
    let Some((name, decidable, decides)) = event_type(&argument.ty) else {
        return Err(Error::new_spanned(&argument.ty, NOT_AN_EVENT));
    };
    if decides && !decidable {
        return Err(Error::new_spanned(&argument.ty, NOT_DECIDABLE));
    }
    Ok((Ident::new(name, Span::call_site()), decides))
}

fn event_type(ty: &Type) -> Option<(&'static str, bool, bool)> {
    let (ident, mutable) = referenced(ty)?;
    let (name, decidable) = EVENTS.iter().find(|(name, _)| ident == name)?;
    Some((*name, *decidable, mutable))
}

fn referenced(ty: &Type) -> Option<(&Ident, bool)> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    let Type::Path(path) = &*reference.elem else {
        return None;
    };
    let segment = path
        .path
        .segments
        .last()
        .filter(|segment| path.qself.is_none() && segment.arguments.is_none())?;
    Some((&segment.ident, reference.mutability.is_some()))
}

fn spec(handler: &Handler) -> TokenStream {
    let Handler {
        cfgs,
        id,
        kind,
        decides,
        priority,
        ignore_cancelled,
        ..
    } = handler;
    let id = Literal::u32_unsuffixed(*id);
    let access = if *decides {
        quote!(Decide)
    } else {
        quote!(Observe)
    };
    quote! {
        #(#cfgs)*
        ::rspigot_plugin::HandlerSpec {
            id: #id,
            event: ::rspigot_plugin::EventKind::#kind,
            access: ::rspigot_plugin::Access::#access,
            priority: ::rspigot_plugin::Priority::#priority,
            ignore_cancelled: #ignore_cancelled,
        }
    }
}

fn arm(handler: &Handler) -> TokenStream {
    let Handler {
        cfgs,
        id,
        method,
        kind,
        ..
    } = handler;
    let id = Literal::u32_unsuffixed(*id);
    if handler.decides {
        quote! {
            #(#cfgs)*
            (#id, ::rspigot_plugin::Call::#kind { mut event, .. }) => {
                self.#method(&mut event).await;
                ::rspigot_plugin::Outcome::Decided(::rspigot_plugin::Verdict::from(event))
            }
        }
    } else {
        quote! {
            #(#cfgs)*
            (#id, ::rspigot_plugin::Call::#kind { event, .. }) => {
                self.#method(&event).await;
                ::rspigot_plugin::Outcome::Observed
            }
        }
    }
}
