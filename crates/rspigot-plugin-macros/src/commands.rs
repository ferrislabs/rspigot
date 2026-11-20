use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, FnArg, Ident, ImplItemFn, LitStr, Pat, Path, Receiver, Token, Type};

const BAD_COMMAND: &str = "a command is `async fn name(&self, ctx: Context, argument: Type, ...)`; the argument types give the command's tree";
const BAD_NAME: &str = "a command name is lowercase words: the command, then its literal sub-commands (`\"shop buy\"`)";
const BAD_ATTRIBUTE: &str = "`#[command(\"name\")]`, optionally with `permission = \"node\"` and `aliases = [\"a\", \"b\"]`";
const BAD_SUGGEST: &str = "`#[suggest(path)]` names an `async fn(&self, &Player, &str) -> Vec<String>`, for example `#[suggest(Self::names)]`";

pub(crate) struct Command {
    pub(crate) cfgs: Vec<Attribute>,
    pub(crate) id: u32,
    pub(crate) method: Ident,
    pub(crate) root: String,
    pub(crate) path: Vec<String>,
    pub(crate) aliases: Vec<LitStr>,
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
    aliases: Vec<LitStr>,
}

impl Parse for Marked {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: LitStr = input.parse()?;
        let mut permission = None;
        let mut aliases = Vec::new();
        while input.parse::<Option<Token![,]>>()?.is_some() && !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if key == "permission" && permission.is_none() {
                permission = Some(input.parse()?);
            } else if key == "aliases" && aliases.is_empty() {
                let list;
                syn::bracketed!(list in input);
                aliases = list
                    .parse_terminated(<LitStr as Parse>::parse, Token![,])?
                    .into_iter()
                    .collect();
            } else {
                return Err(Error::new_spanned(key, BAD_ATTRIBUTE));
            }
        }
        Ok(Self {
            name,
            permission,
            aliases,
        })
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
    let mut words = name.split_whitespace().map(str::to_owned);
    let root = words.next().unwrap_or_default();
    let path: Vec<String> = words.collect();
    if root.is_empty() || name != name.to_lowercase() || name.trim() != name {
        errors.push(Error::new_spanned(&marked.name, BAD_NAME));
        return None;
    }
    if let Some(alias) = marked
        .aliases
        .iter()
        .find(|alias| alias.value().is_empty() || alias.value().contains(char::is_whitespace))
    {
        errors.push(Error::new_spanned(alias, BAD_NAME));
        return None;
    }
    let params = params(method, errors)?;
    Some(Command {
        cfgs,
        id,
        method: method.sig.ident.clone(),
        root,
        path,
        aliases: marked.aliases,
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

pub(crate) fn specs(commands: &[Command]) -> Vec<TokenStream> {
    commands
        .iter()
        .map(|command| {
            let Command {
                cfgs,
                id,
                root,
                path,
                aliases,
                permission,
                params,
                ..
            } = command;
            let id = Literal::u32_unsuffixed(*id);
            let permission = permission.as_ref().map_or_else(
                || quote!(::core::option::Option::None),
                |node| quote!(::core::option::Option::Some(#node)),
            );
            let literals = path.iter().map(|word| {
                quote! {
                    ::rspigot_plugin::CommandArgument {
                        name: #word,
                        kind: ::rspigot_plugin::ArgumentKind::Literal,
                        optional: false,
                        suggested: false,
                        choices: &[],
                    }
                }
            });
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
                        choices: <#ty as ::rspigot_plugin::Argument>::CHOICES,
                    }
                }
            });
            quote! {
                #(#cfgs)*
                ::rspigot_plugin::CommandSpec {
                    id: #id,
                    name: #root,
                    aliases: &[#(#aliases),*],
                    permission: #permission,
                    arguments: ::std::vec![#(#literals,)* #(#arguments),*],
                }
            }
        })
        .collect()
}

pub(crate) fn invoke_match(commands: &[Command], fallback: bool) -> TokenStream {
    let rest = if fallback {
        quote!(_ => self.on_command(invocation).await,)
    } else {
        quote!(_ => {})
    };
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
        match command {
            #(#arms)*
            #rest
        }
    }
}

pub(crate) fn suggests(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| command.params.iter().any(|param| param.suggest.is_some()))
}

pub(crate) fn suggestion_spec(tab: &TokenStream) -> TokenStream {
    quote! {
        ::rspigot_plugin::HandlerSpec {
            id: #tab,
            event: ::rspigot_plugin::EventKind::TabComplete,
            access: ::rspigot_plugin::Access::Decide,
            priority: ::rspigot_plugin::Priority::Normal,
            ignore_cancelled: false,
        }
    }
}

pub(crate) fn suggestion_arm(commands: &[Command], tab: &TokenStream) -> TokenStream {
    let routes = commands.iter().flat_map(|command| {
        let cfgs = &command.cfgs;
        let root = &command.root;
        let aliases: Vec<&LitStr> = commands
            .iter()
            .filter(|other| other.root == command.root)
            .flat_map(|other| other.aliases.iter())
            .collect();
        let path = &command.path;
        let depth = path.len();
        command
            .params
            .iter()
            .enumerate()
            .filter_map(move |(index, param)| {
                param.suggest.as_ref().map(|suggest| {
                    let earlier = command.params[..index].iter().map(|earlier| &earlier.ty);
                    let ty = &param.ty;
                    let aliases = aliases.clone();
                    quote! {
                        #(#cfgs)*
                        if found.is_none()
                            && typing.is_for(
                                &[#root #(, #aliases)*],
                                &[#(#path),*],
                                #depth #(+ <#earlier as ::rspigot_plugin::Argument>::WIDTH)*,
                                <#ty as ::rspigot_plugin::Argument>::WIDTH,
                                <#ty as ::rspigot_plugin::Argument>::KIND
                                    == ::rspigot_plugin::ArgumentKind::GreedyText,
                            )
                        {
                            found = ::core::option::Option::Some(
                                #suggest(self, &typing.player, &typing.text).await
                            );
                        }
                    }
                })
            })
    });
    quote! {
        (__handler, ::rspigot_plugin::Call::TabComplete { mut event, .. }) if __handler == #tab => {
            let buffer = event.buffer().to_owned();
            if let ::core::option::Option::Some(typing) =
                ::rspigot_plugin::Typing::of(&buffer, event.sender())
            {
                let mut found: ::core::option::Option<::std::vec::Vec<::std::string::String>> =
                    ::core::option::Option::None;
                #(#routes)*
                if let ::core::option::Option::Some(values) = found {
                    event.set_completions(values);
                }
            }
            ::rspigot_plugin::Outcome::Decided(::rspigot_plugin::Verdict::from(event))
        }
    }
}
