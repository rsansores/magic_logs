//! `#[derive(MagicLog)]`: a row says what each of its columns is to a log
//! explorer, and the derive writes the [`Spec`](../magic_logs/struct.Spec.html).

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitStr, Type};

/// See the `magic_logs` crate's README for the attributes.
#[proc_macro_derive(MagicLog, attributes(log))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[derive(Default)]
struct Table {
    name: Option<LitStr>,
    schema: Option<LitStr>,
    view_schema: Option<LitStr>,
    chunk: Option<LitStr>,
    compress_after: Option<LitStr>,
    segment_by: Option<LitStr>,
    search_with: Option<LitStr>,
    krate: Option<LitStr>,
}

#[derive(Default)]
struct Flags {
    time: bool,
    id: bool,
    tenant: bool,
    scope: bool,
    level: bool,
    top: bool,
    facet: bool,
    per_top: bool,
    filter: bool,
    indexed: bool,
    search: bool,
    sql: Option<LitStr>,
}

struct Col {
    name: String,
    sql: String,
    flags: Flags,
    at: Span,
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let mut table = Table::default();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("log")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            let slot = match key.as_str() {
                "name" => &mut table.name,
                "schema" => &mut table.schema,
                "view_schema" => &mut table.view_schema,
                "chunk" => &mut table.chunk,
                "compress_after" => &mut table.compress_after,
                "segment_by" => &mut table.segment_by,
                "search_with" => &mut table.search_with,
                "crate" => &mut table.krate,
                _ => {
                    return Err(meta.error(
                        "expected `name`, `schema`, `view_schema`, `chunk`, `compress_after`, \
                         `segment_by`, `search_with` or `crate`",
                    ))
                }
            };
            if slot.is_some() {
                return Err(meta.error(format!("`{key}` is given twice")));
            }
            *slot = Some(meta.value()?.parse()?);
            Ok(())
        })?;
    }
    let required = |lit: &Option<LitStr>, what: &str| {
        lit.as_ref().map(LitStr::value).ok_or_else(|| {
            syn::Error::new_spanned(ident, format!("#[log({what} = \"…\")] is required"))
        })
    };
    let krate: syn::Path = match &table.krate {
        Some(lit) => lit.parse().map_err(|_| {
            syn::Error::new_spanned(lit, "`crate` is a path to magic_logs: \"crate::logs\"")
        })?,
        None => syn::parse_quote!(::magic_logs),
    };
    let name = required(&table.name, "name")?;
    let schema = required(&table.schema, "schema")?;
    let view_schema = table
        .view_schema
        .as_ref()
        .map_or("public".to_owned(), LitStr::value);
    let chunk = table
        .chunk
        .as_ref()
        .map_or("1 day".to_owned(), LitStr::value);
    let compress_after = table
        .compress_after
        .as_ref()
        .map_or("2 days".to_owned(), LitStr::value);

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a log table has no generic parameters",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(ident, "a log table is a struct"));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "a log table's fields are named",
        ));
    };
    let mut cols = Vec::new();
    for field in &named.named {
        let mut flags = Flags::default();
        let mut any = false;
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("log")) {
            any = true;
            attr.parse_nested_meta(|meta| {
                let key = meta
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let flag =
                    match key.as_str() {
                        "time" => &mut flags.time,
                        "id" => &mut flags.id,
                        "tenant" => &mut flags.tenant,
                        "scope" => &mut flags.scope,
                        "level" => &mut flags.level,
                        "top" => &mut flags.top,
                        "facet" => &mut flags.facet,
                        "per_top" => &mut flags.per_top,
                        "filter" => &mut flags.filter,
                        "indexed" => &mut flags.indexed,
                        "search" => &mut flags.search,
                        "sql" => {
                            flags.sql = Some(meta.value()?.parse()?);
                            return Ok(());
                        }
                        _ => return Err(meta.error(
                            "expected `time`, `id`, `tenant`, `scope`, `level`, `top`, `facet`, \
                             `per_top`, `filter`, `indexed`, `search` or `sql = \"…\"`",
                        )),
                    };
                *flag = true;
                Ok(())
            })?;
        }
        if !any {
            continue;
        }
        let at = field
            .ident
            .as_ref()
            .map_or(Span::call_site(), syn::Ident::span);
        if flags.per_top && !flags.facet {
            return Err(syn::Error::new(at, "`per_top` qualifies a `facet`"));
        }
        let sql = match &flags.sql {
            Some(lit) => lit.value(),
            None => sql_of(&field.ty).ok_or_else(|| {
                syn::Error::new_spanned(
                    &field.ty,
                    "no SQL type is known for this type: say it with #[log(sql = \"…\")]",
                )
            })?,
        };
        cols.push(Col {
            name: field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            sql,
            flags,
            at,
        });
    }

    let one = |pick: fn(&Flags) -> bool, what: &str, required: bool| -> syn::Result<Option<&Col>> {
        let found: Vec<&Col> = cols.iter().filter(|c| pick(&c.flags)).collect();
        match found.as_slice() {
            [] if required => Err(syn::Error::new_spanned(
                ident,
                format!("one field is #[log({what})]"),
            )),
            [] => Ok(None),
            [c] => Ok(Some(c)),
            [_, second, ..] => Err(syn::Error::new(
                second.at,
                format!("only one field is #[log({what})]"),
            )),
        }
    };
    let time = one(|f| f.time, "time", true)?.expect("required");
    let id = one(|f| f.id, "id", true)?.expect("required");
    let tenant = one(|f| f.tenant, "tenant", false)?;
    let scope = one(|f| f.scope, "scope", false)?;
    let level = one(|f| f.level, "level", false)?;
    let top = one(|f| f.top, "top", false)?;
    if let Some(segment) = &table.segment_by {
        if !cols.iter().any(|c| c.name == segment.value()) {
            return Err(syn::Error::new_spanned(
                segment,
                "segment_by names a field marked #[log(…)]",
            ));
        }
    }

    let column = |c: &Col| {
        let (n, s) = (&c.name, &c.sql);
        quote!(#krate::Column { name: #n, sql: #s })
    };
    let option = |c: Option<&Col>| match c {
        Some(c) => {
            let col = column(c);
            quote!(::core::option::Option::Some(#col))
        }
        None => quote!(::core::option::Option::None),
    };
    let option_name = |c: Option<&Col>| match c {
        Some(c) => {
            let n = &c.name;
            quote!(::core::option::Option::Some(#n))
        }
        None => quote!(::core::option::Option::None),
    };
    let fields = cols
        .iter()
        .filter(|c| c.flags.facet || c.flags.filter || c.flags.level || c.flags.scope || c.flags.top)
        .map(|c| {
            let col = column(c);
            let counted = c.flags.facet || c.flags.level || c.flags.scope;
            let per_top = c.flags.per_top || c.flags.level || c.flags.scope;
            let indexed = c.flags.indexed;
            quote!(#krate::Field { column: #col, counted: #counted, per_top: #per_top, indexed: #indexed })
        });
    let search = cols
        .iter()
        .filter(|c| c.flags.search)
        .map(|c| c.name.clone());
    let opt_lit = |lit: &Option<LitStr>| match lit {
        Some(l) => quote!(::core::option::Option::Some(#l)),
        None => quote!(::core::option::Option::None),
    };
    let (time_name, id_col) = (&time.name, column(id));
    let (tenant, scope, level, top) = (
        option_name(tenant),
        option_name(scope),
        option(level),
        option(top),
    );
    let (segment_by, search_with) = (opt_lit(&table.segment_by), opt_lit(&table.search_with));

    Ok(quote! {
        impl #krate::MagicLog for #ident {
            const SPEC: #krate::Spec = #krate::Spec {
                name: #name,
                schema: #schema,
                view_schema: #view_schema,
                time: #time_name,
                id: #id_col,
                tenant: #tenant,
                scope: #scope,
                level: #level,
                top: #top,
                fields: &[#(#fields),*],
                search: &[#(#search),*],
                search_with: #search_with,
                storage: #krate::Storage {
                    chunk: #chunk,
                    compress_after: #compress_after,
                    segment_by: #segment_by,
                },
            };
        }
    })
}

/// The SQL type a Rust type is read as, through an `Option`.
fn sql_of(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    let name = last.ident.to_string();
    if name == "Option" {
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                return sql_of(inner);
            }
        }
        return None;
    }
    Some(
        match name.as_str() {
            "Uuid" => "uuid",
            "String" => "text",
            "DateTime" => "timestamptz",
            "i64" => "bigint",
            "i32" => "integer",
            "i16" => "smallint",
            "bool" => "boolean",
            "f64" => "double precision",
            _ => return None,
        }
        .to_owned(),
    )
}
