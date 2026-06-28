use datex_macro_utils::entrypoint::{
    DatexMainInput, ParsedAttributes, datex_main_impl,
};
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input, parse_quote};

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
        init: Some(quote! {
            datex::flexi_logger::Logger::try_with_env_or_str("warn")
                   .unwrap_or_else(|_e| datex::flexi_logger::Logger::with(datex::flexi_logger::LogSpecification::warn()))
                   .log_to_stderr()
                   .start()
                   .ok();
            datex::com_interfaces::register_native_interface_factories(&runtime.com_hub());
        }),
        pre_body: None,
        additional_attributes: vec![parse_quote! {#[tokio::main]}],
        custom_main_inputs: vec![],
        enforce_main_name: true,
    }).into()
}
