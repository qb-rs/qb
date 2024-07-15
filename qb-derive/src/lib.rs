use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

#[proc_macro_derive(QBIAsync, attributes(context))]
pub fn derive_qbi_async(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;
    let generics = &input.attrs[0].parse_args::<Ident>().unwrap();

    let expanded = quote! {
        // The generated impl.
        impl qb::QBI<#generics> for #name {
            fn init(cx: #generics, com: qb::QBICommunication) -> Self{
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(#name::init_async(cx, com))
            }

            fn run(self) {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(self.run_async());
            }
        }
    };

    // Hand the output tokens back to the compiler.
    proc_macro::TokenStream::from(expanded)
}
