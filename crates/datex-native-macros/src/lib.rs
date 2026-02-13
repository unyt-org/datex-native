use proc_macro::TokenStream;
use syn::{parse_macro_input, parse_quote, ItemFn};
use datex_core::macro_utils::entrypoint::{datex_main_impl, DatexMainInput, ParsedAttributes};

/// The main entry point for a DATEX application, providing a DATEX runtime instance
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed_attributes = parse_macro_input!(attr as ParsedAttributes);

    let original_function = parse_macro_input!(item as ItemFn);
    datex_main_impl(DatexMainInput {
        parsed_attributes,
        func: original_function,
        datex_core_namespace: "datex::core",
        setup: None,
        init: None,
        additional_attributes: vec![parse_quote! {#[tokio::main]}],
        custom_main_inputs: vec![],
        enforce_main_name: false,
    }).into()
}
