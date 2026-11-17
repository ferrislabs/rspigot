use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, FnArg, Ident, ImplItemFn, LitStr, Pat, Path, Receiver, Token, Type};

const BAD_COMMAND: &str = "a command is `async fn name(&self, ctx: Context, argument: Type, ...)`; the argument types give the command's tree";
const BAD_NAME: &str = "a command name is lowercase, without spaces";
const BAD_ATTRIBUTE: &str =
    "`#[command(\"name\")]` or `#[command(\"name\", permission = \"node\")]`";
const BAD_SUGGEST: &str = "`#[suggest(path)]` names an `async fn(&self, &Player, &str) -> Vec<String>`, for example `#[suggest(Self::names)]`";

pub(crate) struct Command {
    pub(crate) cfgs: Vec<Attribute>,
    pub(crate) id: u32,
    pub(crate) method: Ident,
    pub(crate) name: LitStr,
    pub(crate) permission: Option<LitStr>,
    pub(crate) params: Vec<Param>,
}

pub(crate) struct Param {
    pub(crate) ident: Ident,
    pub(crate) ty: Type,
    pub(crate) suggest: Option<Path>,
}

struct Marked {
    name: LitStr,
    permission: Option<LitStr>,
}

impl Parse for Marked {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: LitStr = input.parse()?;
        let mut permission = None;
        if input.parse::<Option<Token![,]>>()?.is_some() && !input.is_empty() {
            let key: Ident = input.parse()?;
            if key != "permission" {
                return Err(Error::new_spanned(key, BAD_ATTRIBUTE));
            }
            input.parse::<Token![=]>()?;
            permission = Some(input.parse()?);
            input.parse::<Option<Token![,]>>()?;
        }
        Ok(Self { name, permission })
    }
}

pub(crate) fn marked(method: &ImplItemFn) -> bool {
    method
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("command"))
}

pub(crate) fn read(
    method: &mut ImplItemFn,
    id: u32,
    cfgs: Vec<Attribute>,
    errors: &mut Vec<Error>,
) -> Option<Command> {
    let mut marks = Vec::new();
    method.attrs.retain(|attr| {
        let mark = attr.path().is_ident("command");
        if mark {
            marks.push(attr.clone());
        }
        !mark
    });
    let [mark] = marks.as_slice() else {
        errors.push(Error::new_spanned(
            &method.sig,
            "a method carries one `#[command(...)]`",
        ));
        return None;
    };
    let marked: Marked = match mark.parse_args() {
        Ok(marked) => marked,
        Err(error) => {
            errors.push(error);
            return None;
        }
    };
    let name = marked.name.value();
    if name.is_empty() || name.contains(char::is_whitespace) || name != name.to_lowercase() {
        errors.push(Error::new_spanned(&marked.name, BAD_NAME));
        return None;
    }
    let params = params(method, errors)?;
    Some(Command {
        cfgs,
        id,
        method: method.sig.ident.clone(),
        name: marked.name,
        permission: marked.permission,
        params,
    })
}

fn params(method: &mut ImplItemFn, errors: &mut Vec<Error>) -> Option<Vec<Param>> {
    let sig = &mut method.sig;
    let shaped = sig.asyncness.is_some()
        && sig.generics.params.is_empty()
        && matches!(
            sig.inputs.first(),
            Some(FnArg::Receiver(Receiver {
                reference: Some(_),
                mutability: None,
                ..
            }))
        )
        && matches!(sig.inputs.iter().nth(1), Some(FnArg::Typed(_)));
    if !shaped {
        errors.push(Error::new_spanned(&sig.inputs, BAD_COMMAND));
        return None;
    }
    let mut params = Vec::new();
    let mut valid = true;
    for argument in sig.inputs.iter_mut().skip(2) {
        let FnArg::Typed(argument) = argument else {
            continue;
        };
        let mut suggest = None;
        argument.attrs.retain(|attr| {
            if !attr.path().is_ident("suggest") {
                return true;
            }
            match attr.parse_args::<Path>() {
                Ok(path) => suggest = Some(path),
                Err(_) => {
                    errors.push(Error::new_spanned(attr, BAD_SUGGEST));
                    valid = false;
                }
            }
            false
        });
        let Pat::Ident(pattern) = &*argument.pat else {
            errors.push(Error::new_spanned(&argument.pat, BAD_COMMAND));
            valid = false;
            continue;
        };
        params.push(Param {
            ident: pattern.ident.clone(),
            ty: (*argument.ty).clone(),
            suggest,
        });
    }
    valid.then_some(params)
}

pub(crate) fn specs(commands: &[Command]) -> TokenStream {
    let specs = commands.iter().map(|command| {
        let Command {
            cfgs,
            id,
            name,
            permission,
            params,
            ..
        } = command;
        let id = Literal::u32_unsuffixed(*id);
        let permission = permission.as_ref().map_or_else(
            || quote!(::core::option::Option::None),
            |node| quote!(::core::option::Option::Some(#node)),
        );
        let arguments = params.iter().map(|param| {
            let label = param.ident.to_string();
            let ty = &param.ty;
            let suggested = param.suggest.is_some();
            quote! {
                ::rspigot_plugin::CommandArgument {
                    name: #label,
                    kind: <#ty as ::rspigot_plugin::Argument>::KIND,
                    optional: <#ty as ::rspigot_plugin::Argument>::OPTIONAL,
                    suggested: #suggested,
                }
            }
        });
        quote! {
            #(#cfgs)*
            ::rspigot_plugin::CommandSpec {
                id: #id,
                name: #name,
                permission: #permission,
                arguments: ::std::vec![#(#arguments),*],
            }
        }
    });
    quote! {
        fn commands() -> ::std::vec::Vec<::rspigot_plugin::CommandSpec> {
            ::std::vec![#(#specs),*]
        }
    }
}

pub(crate) fn invoke(commands: &[Command]) -> TokenStream {
    let arms = commands.iter().map(|command| {
        let Command {
            cfgs,
            id,
            method,
            params,
            ..
        } = command;
        let id = Literal::u32_unsuffixed(*id);
        let names: Vec<&Ident> = params.iter().map(|param| &param.ident).collect();
        let types = params.iter().map(|param| &param.ty);
        quote! {
            #(#cfgs)*
            #id => {
                let ::core::option::Option::Some((context, mut arguments)) = invocation.open() else {
                    return;
                };
                #(
                    let ::core::option::Option::Some(#names) = arguments.read::<#types>(&context) else {
                        return;
                    };
                )*
                self.#method(context, #(#names),*).await;
            }
        }
    });
    quote! {
        async fn invoke(&self, invocation: ::rspigot_plugin::Invocation) {
            match invocation.command() {
                #(#arms)*
                _ => {}
            }
        }
    }
}

pub(crate) fn suggests(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| command.params.iter().any(|param| param.suggest.is_some()))
}

pub(crate) fn suggestion_spec() -> TokenStream {
    quote! {
        ::rspigot_plugin::HandlerSpec {
            id: ::core::primitive::u32::MAX,
            event: ::rspigot_plugin::EventKind::TabComplete,
            access: ::rspigot_plugin::Access::Decide,
            priority: ::rspigot_plugin::Priority::Normal,
            ignore_cancelled: false,
        }
    }
}

pub(crate) fn suggestion_arm(commands: &[Command]) -> TokenStream {
    let routes = commands.iter().flat_map(|command| {
        let cfgs = &command.cfgs;
        let name = &command.name;
        command
            .params
            .iter()
            .enumerate()
            .filter_map(move |(index, param)| {
                param.suggest.as_ref().map(|path| {
                    quote! {
                        #(#cfgs)*
                        (#name, #index) => ::core::option::Option::Some(
                            #path(self, typing.player(), typing.text()).await
                        ),
                    }
                })
            })
    });
    quote! {
        ::rspigot_plugin::Call::TabComplete { handler: ::core::primitive::u32::MAX, mut event } => {
            let buffer = event.buffer().to_owned();
            if let ::core::option::Option::Some(typing) =
                ::rspigot_plugin::Typing::of(&buffer, event.sender())
            {
                let found: ::core::option::Option<::std::vec::Vec<::std::string::String>> =
                    match (typing.command(), typing.position()) {
                        #(#routes)*
                        _ => ::core::option::Option::None,
                    };
                if let ::core::option::Option::Some(values) = found {
                    event.set_completions(values);
                }
            }
            ::rspigot_plugin::Outcome::Decided(::rspigot_plugin::Verdict::from(event))
        }
    }
}
