use quote::quote;

use super::simulation_aspects::*;

// ##################################### DERIVE ######################################
// ##################################### PARSING #####################################
struct Communicator {
    struct_name: syn::Ident,
    generics: syn::Generics,
    comms: Vec<CommField>,
}

struct CommParser {
    _comm_ident: syn::Ident,
    index: syn::Type,
    _comma: syn::Token![,],
    message: syn::Type,
    _comma_2: syn::Token![,],
    core_path: syn::Path,
}

struct CommField {
    field_name: Option<syn::Ident>,
    field_type: syn::Type,
    index: syn::Type,
    message: syn::Type,
    core_path: syn::Path,
}

impl syn::parse::Parse for Communicator {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let item_struct: syn::ItemStruct = input.parse()?;
        let comms = item_struct
            .fields
            .iter()
            .map(|field| field.attrs.iter().zip(std::iter::repeat(field)))
            .flatten()
            .map(|(attr, field)| {
                let s = &attr.meta;
                let stream: proc_macro::TokenStream = quote!(#s).into();
                let parsed: CommParser = syn::parse(stream)?;
                Ok(CommField {
                    field_name: field.ident.clone(),
                    field_type: field.ty.clone(),
                    index: parsed.index,
                    message: parsed.message,
                    core_path: parsed.core_path,
                })
            })
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(Self {
            struct_name: item_struct.ident,
            generics: item_struct.generics,
            comms,
        })
    }
}

impl syn::parse::Parse for CommParser {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _comm_ident = input.parse()?;
        let content;
        syn::parenthesized!(content in input);
        Ok(Self {
            _comm_ident,
            index: content.parse()?,
            _comma: content.parse()?,
            message: content.parse()?,
            _comma_2: content.parse()?,
            core_path: content.parse()?,
        })
    }
}

// ################################### IMPLEMENTING ##################################
fn wrap_pre_flags(
    core_path: &proc_macro2::TokenStream,
    stream: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote!(
        #[allow(unused)]
        #[allow(non_camel_case_types)]
        const _: () = {
            use #core_path ::backend::chili::{
                errors::SimulationError,
                simulation_flow::Communicator
            };
            use #core_path ::derive::Communicator;

            #stream
        };
    )
}

impl Communicator {
    fn derive_communicator(&self) -> proc_macro2::TokenStream {
        let struct_name = &self.struct_name;

        let (impl_generics, ty_generics, where_clause) = &self.generics.split_for_impl();
        let addendum = quote!(I: Clone + core::hash::Hash + Eq + Ord,);
        let where_clause = match where_clause {
            Some(w) => quote!(where #(#w.predicates), #addendum),
            None => quote!(where #addendum),
        };

        let mut res = proc_macro2::TokenStream::new();
        res.extend(self.comms.iter().map(|comm| {
            let field_name = &comm.field_name;
            let field_type = &comm.field_type;

            let core_path = &comm.core_path;
            let flow_path = quote!(#core_path ::backend::chili::simulation_flow::);
            let error_path = quote!(#core_path ::backend::chili::errors::);

            let index = &comm.index;
            let message = &comm.message;

            wrap_pre_flags(&quote!(#core_path), quote!(
                #[automatically_derived]
                impl #impl_generics #flow_path Communicator<#index, #message>
                for #struct_name #ty_generics #where_clause

                {
                    fn send(&mut self, receiver: &#index, message: #message) -> Result<(), #error_path SimulationError> {
                        <#field_type as #flow_path Communicator<#index, #message>>::send(&mut self.#field_name, receiver, message)
                    }
                    fn receive(&mut self) -> Vec<#message> {
                        <#field_type as #flow_path Communicator<#index, #message>>::receive(&mut self.#field_name)
                    }
                }
            ))
        }));
        res
    }
}

pub fn derive_communicator(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let comm = syn::parse_macro_input!(input as Communicator);
    let stream = comm.derive_communicator();

    proc_macro::TokenStream::from(stream)
}

// ################################### CONSTRUCTING ##################################
struct ConstructInput {
    name_def: NameDefinition,
    _comma_1: syn::Token![,],
    aspects: SimulationAspects,
    _comma_2: syn::Token![,],
    sim_flow_path: SimFlowPath,
    _comma_3: Option<syn::Token![,]>,
    core_path: Option<CorePath>,
}

impl syn::parse::Parse for ConstructInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name_def: input.parse()?,
            _comma_1: input.parse()?,
            aspects: input.parse()?,
            _comma_2: input.parse()?,
            sim_flow_path: input.parse()?,
            _comma_3: input.parse()?,
            core_path: if input.is_empty() {
                None
            } else {
                Some(input.parse::<CorePath>()?)
            },
        })
    }
}

impl SimulationAspect {
    fn build_comm(
        &self,
        sim_flow_path: &syn::Path,
        core_path: &proc_macro2::TokenStream,
    ) -> (Vec<syn::Type>, Vec<proc_macro2::TokenStream>) {
        match self {
            SimulationAspect::Cycle => (vec![], vec![]),
            SimulationAspect::Reactions => (vec![], vec![]),
            SimulationAspect::Mechanics => (
                vec![
                    syn::parse2(quote!(I)).unwrap(),
                    syn::parse2(quote!(Cel)).unwrap(),
                    syn::parse2(quote!(Aux)).unwrap(),
                ],
                vec![quote!(
                    #[Comm(I, #sim_flow_path ::SendCell<Cel, Aux>, #core_path)]
                    comm_cell: #sim_flow_path ::ChannelComm<I, #sim_flow_path ::SendCell<Cel, Aux>>
                )],
            ),
            SimulationAspect::Interaction => (
                vec![
                    syn::parse2(quote!(I)).unwrap(),
                    syn::parse2(quote!(Pos)).unwrap(),
                    syn::parse2(quote!(Vel)).unwrap(),
                    syn::parse2(quote!(For)).unwrap(),
                    syn::parse2(quote!(Inf)).unwrap(),
                ],
                vec![
                    quote!(
                        #[Comm(I, #sim_flow_path ::PosInformation<Pos, Vel, Inf>, #core_path)]
                        comm_pos: #sim_flow_path ::ChannelComm<I, #sim_flow_path ::PosInformation<Pos, Vel, Inf>>
                    ),
                    quote!(
                        #[Comm(I, #sim_flow_path ::ForceInformation<For>, #core_path)]
                        comm_force: #sim_flow_path ::ChannelComm<I, #sim_flow_path ::ForceInformation<For>>
                    ),
                ],
            ),
        }
    }
}

impl ConstructInput {
    fn build_communicator(self) -> proc_macro2::TokenStream {
        let struct_name = self.name_def.struct_name;
        let core_path = match self.core_path {
            Some(path) => {
                let p = path.path;
                quote!(#p)
            }
            None => quote!(cellular_raza::core),
        };
        let generics_fields: Vec<_> = self
            .aspects
            .items
            .into_iter()
            .map(|aspect| aspect.build_comm(&self.sim_flow_path.path, &core_path))
            .collect();

        let mut generics = vec![];
        let mut fields = vec![];

        generics_fields.into_iter().for_each(|(g, f)| {
            g.into_iter().for_each(|gi| {
                if !generics.contains(&gi) {
                    generics.push(gi);
                }
            });
            fields.extend(f);
        });
        quote!(
            #[derive(#core_path ::derive::Communicator)]
            #[allow(non_camel_case_types)]
            struct #struct_name <#(#generics),*> {
                #(#fields),*
            }
        )
    }
}

pub fn construct_communicator(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let constr = syn::parse_macro_input!(input as ConstructInput);
    let stream = constr.build_communicator();
    proc_macro::TokenStream::from(stream)
}
