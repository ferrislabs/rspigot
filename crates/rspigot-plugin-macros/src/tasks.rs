use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{Attribute, Error, FnArg, Ident, ImplItemFn, LitStr, Receiver, ReturnType};

use crate::listeners::{cfgs, referenced};

const BAD_TASK: &str = "a task is `async fn name(&self)` or `async fn name(&self, server: &Server)`, declared `#[task(every = \"30s\")]`";
const BAD_EVERY: &str = "`every` is a number and a unit, `\"500ms\"`, `\"30s\"`, `\"5m\"` or `\"1h\"`, of at least 100ms";
const MINIMUM_MILLIS: u64 = 100;

pub(crate) struct Task {
    pub(crate) cfgs: Vec<Attribute>,
    pub(crate) method: Ident,
    pub(crate) millis: u64,
    pub(crate) takes_server: bool,
}

pub(crate) fn marked(method: &ImplItemFn) -> bool {
    method.attrs.iter().any(|attr| attr.path().is_ident("task"))
}

pub(crate) fn read(method: &mut ImplItemFn, errors: &mut Vec<Error>) -> Option<Task> {
    let mut every = None;
    for attr in std::mem::take(&mut method.attrs) {
        if !attr.path().is_ident("task") {
            method.attrs.push(attr);
            continue;
        }
        if every.is_some() {
            errors.push(Error::new_spanned(
                &attr,
                "this method already has a `#[task]`",
            ));
            continue;
        }
        match parse_every(&attr) {
            Ok(millis) => every = Some(millis),
            Err(error) => errors.push(error),
        }
    }
    let millis = every?;
    let sig = &method.sig;
    let mut inputs = sig.inputs.iter();
    let by_ref = matches!(
        inputs.next(),
        Some(FnArg::Receiver(Receiver {
            reference: Some(_),
            mutability: None,
            ..
        }))
    );
    let takes_server = match inputs.next() {
        None => Some(false),
        Some(FnArg::Typed(argument)) if matches!(referenced(&argument.ty), Some((name, false)) if name == "Server") => {
            Some(true)
        }
        Some(_) => None,
    };
    let plain = sig.asyncness.is_some()
        && sig.generics.params.is_empty()
        && sig.generics.where_clause.is_none()
        && matches!(sig.output, ReturnType::Default);
    match takes_server {
        Some(takes_server) if by_ref && plain && inputs.next().is_none() => Some(Task {
            cfgs: cfgs(&method.attrs),
            method: sig.ident.clone(),
            millis,
            takes_server,
        }),
        _ => {
            errors.push(Error::new_spanned(sig, BAD_TASK));
            None
        }
    }
}

fn parse_every(attr: &Attribute) -> Result<u64, Error> {
    let mut text: Option<LitStr> = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("every") {
            text = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error(BAD_EVERY))
        }
    })?;
    let text = text.ok_or_else(|| Error::new_spanned(attr, BAD_EVERY))?;
    let millis =
        duration_millis(&text.value()).ok_or_else(|| Error::new_spanned(&text, BAD_EVERY))?;
    if millis < MINIMUM_MILLIS {
        return Err(Error::new_spanned(&text, BAD_EVERY));
    }
    Ok(millis)
}

fn duration_millis(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(split);
    let number: u64 = number.parse().ok()?;
    let factor = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return None,
    };
    number.checked_mul(factor)
}

pub(crate) fn runs(tasks: &[Task], owner: &TokenStream) -> Vec<TokenStream> {
    tasks
        .iter()
        .map(|task| {
            let Task {
                cfgs,
                method,
                millis,
                takes_server,
            } = task;
            let millis = Literal::u64_unsuffixed(*millis);
            let key = format!("::{method}");
            let call = if *takes_server {
                quote!(self.#method(server).await;)
            } else {
                quote!(self.#method().await;)
            };
            quote! {
                #(#cfgs)*
                if clock.due(
                    ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#owner), #key),
                    ::std::time::Duration::from_millis(#millis),
                ) {
                    #call
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_have_a_unit() {
        assert_eq!(duration_millis("500ms"), Some(500));
        assert_eq!(duration_millis("30s"), Some(30_000));
        assert_eq!(duration_millis("5m"), Some(300_000));
        assert_eq!(duration_millis("1h"), Some(3_600_000));
        assert_eq!(duration_millis("30"), None);
        assert_eq!(duration_millis("s"), None);
        assert_eq!(duration_millis("2d"), None);
        assert_eq!(duration_millis(""), None);
    }
}
