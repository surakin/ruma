//! Details of the `metadata` section of the procedural macro.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    Ident, LitBool, LitStr, Token,
};

use super::{auth_scheme::AuthScheme, util, version::MatrixVersionLiteral};

mod kw {
    syn::custom_keyword!(metadata);
    syn::custom_keyword!(description);
    syn::custom_keyword!(method);
    syn::custom_keyword!(name);
    syn::custom_keyword!(unstable_path);
    syn::custom_keyword!(r0_path);
    syn::custom_keyword!(stable_path);
    syn::custom_keyword!(rate_limited);
    syn::custom_keyword!(authentication);
    syn::custom_keyword!(added);
    syn::custom_keyword!(deprecated);
    syn::custom_keyword!(removed);
}

/// The result of processing the `metadata` section of the macro.
pub struct Metadata {
    /// The description field.
    pub description: LitStr,

    /// The method field.
    pub method: Ident,

    /// The name field.
    pub name: LitStr,

    /// The rate_limited field.
    pub rate_limited: LitBool,

    /// The authentication field.
    pub authentication: AuthScheme,

    /// The version history field.
    pub history: History,
}

fn set_field<T: ToTokens>(field: &mut Option<T>, value: T) -> syn::Result<()> {
    match field {
        Some(existing_value) => {
            let mut error = syn::Error::new_spanned(value, "duplicate field assignment");
            error.combine(syn::Error::new_spanned(existing_value, "first one here"));
            Err(error)
        }
        None => {
            *field = Some(value);
            Ok(())
        }
    }
}

impl Parse for Metadata {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let metadata_kw: kw::metadata = input.parse()?;
        let _: Token![:] = input.parse()?;

        let field_values;
        braced!(field_values in input);

        let field_values =
            field_values.parse_terminated::<FieldValue, Token![,]>(FieldValue::parse)?;

        let mut description = None;
        let mut method = None;
        let mut name = None;
        let mut unstable_path = None;
        let mut r0_path = None;
        let mut stable_path = None;
        let mut rate_limited = None;
        let mut authentication = None;
        let mut added = None;
        let mut deprecated = None;
        let mut removed = None;

        for field_value in field_values {
            match field_value {
                FieldValue::Description(d) => set_field(&mut description, d)?,
                FieldValue::Method(m) => set_field(&mut method, m)?,
                FieldValue::Name(n) => set_field(&mut name, n)?,
                FieldValue::UnstablePath(p) => set_field(&mut unstable_path, p)?,
                FieldValue::R0Path(p) => set_field(&mut r0_path, p)?,
                FieldValue::StablePath(p) => set_field(&mut stable_path, p)?,
                FieldValue::RateLimited(rl) => set_field(&mut rate_limited, rl)?,
                FieldValue::Authentication(a) => set_field(&mut authentication, a)?,
                FieldValue::Added(v) => set_field(&mut added, v)?,
                FieldValue::Deprecated(v) => set_field(&mut deprecated, v)?,
                FieldValue::Removed(v) => set_field(&mut removed, v)?,
            }
        }

        let missing_field =
            |name| syn::Error::new_spanned(metadata_kw, format!("missing field `{name}`"));

        // Construct the History object.
        let history = {
            let stable_or_r0 = stable_path.as_ref().or(r0_path.as_ref());

            if let Some(path) = stable_or_r0 {
                if added.is_none() {
                    return Err(syn::Error::new_spanned(
                        path,
                        "stable path was defined, while `added` version was not defined",
                    ));
                }
            }

            if let Some(deprecated) = &deprecated {
                if added.is_none() {
                    return Err(syn::Error::new_spanned(
                        deprecated,
                        "deprecated version is defined while added version is not defined",
                    ));
                }
            }

            // Note: It is possible that Matrix will remove endpoints in a single version, while
            // not having a deprecation version inbetween, but that would not be allowed by their
            // own deprecation policy, so lets just assume  there's always a deprecation version
            // before a removal one.
            //
            // If Matrix does so anyways, we can just alter this.
            if let Some(removed) = &removed {
                if deprecated.is_none() {
                    return Err(syn::Error::new_spanned(
                        removed,
                        "removed version is defined while deprecated version is not defined",
                    ));
                }
            }

            if let Some(added) = &added {
                if stable_or_r0.is_none() {
                    return Err(syn::Error::new_spanned(
                        added,
                        "added version is defined, but no stable or r0 path exists",
                    ));
                }
            }

            if let Some(r0) = &r0_path {
                let added =
                    added.as_ref().expect("we error if r0 or stable is defined without added");

                if added.major.get() == 1 && added.minor > 0 {
                    return Err(syn::Error::new_spanned(
                        r0,
                        "r0 defined while added version is newer than v1.0",
                    ));
                }

                if stable_path.is_none() {
                    return Err(syn::Error::new_spanned(r0, "r0 defined without stable path"));
                }

                if !r0.value().contains("/r0/") {
                    return Err(syn::Error::new_spanned(r0, "r0 endpoint does not contain /r0/"));
                }
            }

            if let Some(stable) = &stable_path {
                if stable.value().contains("/r0/") {
                    return Err(syn::Error::new_spanned(
                        stable,
                        "stable endpoint contains /r0/ (did you make a copy-paste error?)",
                    ));
                }
            }

            if unstable_path.is_none() && r0_path.is_none() && stable_path.is_none() {
                return Err(syn::Error::new_spanned(
                    metadata_kw,
                    "need to define one of [r0_path, stable_path, unstable_path]",
                ));
            }

            History::construct(deprecated, removed, unstable_path, r0_path, stable_path.zip(added))
        };

        Ok(Self {
            description: description.ok_or_else(|| missing_field("description"))?,
            method: method.ok_or_else(|| missing_field("method"))?,
            name: name.ok_or_else(|| missing_field("name"))?,
            rate_limited: rate_limited.ok_or_else(|| missing_field("rate_limited"))?,
            authentication: authentication.ok_or_else(|| missing_field("authentication"))?,
            history,
        })
    }
}

enum Field {
    Description,
    Method,
    Name,
    UnstablePath,
    R0Path,
    StablePath,
    RateLimited,
    Authentication,
    Added,
    Deprecated,
    Removed,
}

impl Parse for Field {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(kw::description) {
            let _: kw::description = input.parse()?;
            Ok(Self::Description)
        } else if lookahead.peek(kw::method) {
            let _: kw::method = input.parse()?;
            Ok(Self::Method)
        } else if lookahead.peek(kw::name) {
            let _: kw::name = input.parse()?;
            Ok(Self::Name)
        } else if lookahead.peek(kw::unstable_path) {
            let _: kw::unstable_path = input.parse()?;
            Ok(Self::UnstablePath)
        } else if lookahead.peek(kw::r0_path) {
            let _: kw::r0_path = input.parse()?;
            Ok(Self::R0Path)
        } else if lookahead.peek(kw::stable_path) {
            let _: kw::stable_path = input.parse()?;
            Ok(Self::StablePath)
        } else if lookahead.peek(kw::rate_limited) {
            let _: kw::rate_limited = input.parse()?;
            Ok(Self::RateLimited)
        } else if lookahead.peek(kw::authentication) {
            let _: kw::authentication = input.parse()?;
            Ok(Self::Authentication)
        } else if lookahead.peek(kw::added) {
            let _: kw::added = input.parse()?;
            Ok(Self::Added)
        } else if lookahead.peek(kw::deprecated) {
            let _: kw::deprecated = input.parse()?;
            Ok(Self::Deprecated)
        } else if lookahead.peek(kw::removed) {
            let _: kw::removed = input.parse()?;
            Ok(Self::Removed)
        } else {
            Err(lookahead.error())
        }
    }
}

enum FieldValue {
    Description(LitStr),
    Method(Ident),
    Name(LitStr),
    UnstablePath(EndpointPath),
    R0Path(EndpointPath),
    StablePath(EndpointPath),
    RateLimited(LitBool),
    Authentication(AuthScheme),
    Added(MatrixVersionLiteral),
    Deprecated(MatrixVersionLiteral),
    Removed(MatrixVersionLiteral),
}

impl Parse for FieldValue {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let field: Field = input.parse()?;
        let _: Token![:] = input.parse()?;

        Ok(match field {
            Field::Description => Self::Description(input.parse()?),
            Field::Method => Self::Method(input.parse()?),
            Field::Name => Self::Name(input.parse()?),
            Field::UnstablePath => Self::UnstablePath(input.parse()?),
            Field::R0Path => Self::R0Path(input.parse()?),
            Field::StablePath => Self::StablePath(input.parse()?),
            Field::RateLimited => Self::RateLimited(input.parse()?),
            Field::Authentication => Self::Authentication(input.parse()?),
            Field::Added => Self::Added(input.parse()?),
            Field::Deprecated => Self::Deprecated(input.parse()?),
            Field::Removed => Self::Removed(input.parse()?),
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct History {
    pub(super) entries: Vec<HistoryEntry>,
    misc: MiscVersioning,
}

impl History {
    // TODO(j0j0): remove after codebase conversion is complete
    /// Construct a History object from legacy parts.
    pub fn construct(
        deprecated: Option<MatrixVersionLiteral>,
        removed: Option<MatrixVersionLiteral>,
        unstable_path: Option<EndpointPath>,
        r0_path: Option<EndpointPath>,
        stable_path_and_version: Option<(EndpointPath, MatrixVersionLiteral)>,
    ) -> Self {
        // Unfortunately can't `use` associated constants
        const V1_0: MatrixVersionLiteral = MatrixVersionLiteral::V1_0;

        let unstable = unstable_path.map(|path| HistoryEntry::Unstable { path });
        let r0 = r0_path.map(|path| HistoryEntry::Stable { path, version: V1_0 });
        let stable = stable_path_and_version.map(|(path, mut version)| {
            // If added in 1.0 as r0, the new stable path must be from 1.1
            if r0.is_some() && version == V1_0 {
                version = MatrixVersionLiteral::V1_1;
            }

            HistoryEntry::Stable { path, version }
        });

        let misc = match (deprecated, removed) {
            (None, None) => MiscVersioning::None,
            (Some(deprecated), None) => MiscVersioning::Deprecated(deprecated),
            (Some(deprecated), Some(removed)) => MiscVersioning::Removed { deprecated, removed },

            (None, Some(_)) => unreachable!("removed implies deprecated"),
        };

        let entries = [unstable, r0, stable].into_iter().flatten().collect();

        History { entries, misc }
    }
}

#[derive(Debug, PartialEq)]
pub enum MiscVersioning {
    None,
    Deprecated(MatrixVersionLiteral),
    Removed { deprecated: MatrixVersionLiteral, removed: MatrixVersionLiteral },
}

impl ToTokens for History {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        fn endpointpath_to_pathdata_ts(endpoint: &EndpointPath) -> String {
            endpoint.value()
        }

        let unstable = self.entries.iter().filter_map(|e| match e {
            HistoryEntry::Unstable { path } => Some(endpointpath_to_pathdata_ts(path)),
            _ => None,
        });
        let versioned = self.entries.iter().filter_map(|e| match e {
            HistoryEntry::Stable { path, version } => {
                let path = endpointpath_to_pathdata_ts(path);
                Some(quote! {( #version, #path )})
            }
            _ => None,
        });

        let (deprecated, removed) = match &self.misc {
            MiscVersioning::None => (None, None),
            MiscVersioning::Deprecated(deprecated) => (Some(deprecated), None),
            MiscVersioning::Removed { deprecated, removed } => (Some(deprecated), Some(removed)),
        };

        let deprecated = util::map_option_literal(&deprecated);
        let removed = util::map_option_literal(&removed);

        tokens.extend(quote! {
            ::ruma_common::api::VersionHistory::new(
                &[ #(#unstable),* ],
                &[ #(#versioned),* ],
                #deprecated,
                #removed,
            )
        });
    }
}

#[derive(Debug, PartialEq)]
// Unused variants will be constructed when the macro input is updated
#[allow(dead_code)]
pub enum HistoryEntry {
    Unstable { path: EndpointPath },
    Stable { version: MatrixVersionLiteral, path: EndpointPath },
    Deprecated { version: MatrixVersionLiteral },
    Removed { version: MatrixVersionLiteral },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EndpointPath(LitStr);

impl EndpointPath {
    pub fn value(&self) -> String {
        self.0.value()
    }
}

impl Parse for EndpointPath {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;

        if util::is_valid_endpoint_path(&path.value()) {
            Ok(Self(path))
        } else {
            Err(syn::Error::new_spanned(
                &path,
                "path may only contain printable ASCII characters with no spaces",
            ))
        }
    }
}

impl ToTokens for EndpointPath {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}
