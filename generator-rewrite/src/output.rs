mod vfs;

use crate::output::vfs::VirtualRustFs;
use analysis::{
    LibraryName,
    item::{MultiRequireLocation, RequireLocation, RequiredBy},
};
use heck::ToSnekCase;
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{
    hash::Hash,
    io, iter,
    path::{Path, PathBuf},
};
use syn::Ident;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Destination {
    pub library: LibraryName,
    pub locations: MultiRequireLocation,
    pub reexport: bool,
}

impl Destination {
    pub fn new(required_by: RequiredBy) -> Destination {
        Destination {
            library: required_by.library,
            locations: required_by.locations,
            reexport: true,
        }
    }

    pub fn single_dests(&self) -> Vec<SingleDestination> {
        self.locations.iter().map(|location| {
            SingleDestination {
                library: self.library,
                location: location.clone(),
                reexport: self.reexport // TODO: figure out re-export based on "primary location"
            }
        }).collect::<Vec<_>>()
    }

    pub fn guess_primary(&self) -> SingleDestination {
        // TODO: figure out which is the "primary" location that defined, either from analysis or xml
        // it might not always be the very first index (for example, image_compression_control PFN_vkGetImageSubresourceLayout2 is defined *later* in vk.xml compared to the duplicate fn from host_image_copy, which comes earlier)
        // just because host_image_copy came earlier does NOT mean that it is the primary location. must look at the "required by / extends" chain or something like dat.
        let primary_location = self.locations[0];

        SingleDestination { library: self.library, location: primary_location, reexport: self.reexport }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SingleDestination {
    pub library: LibraryName,
    pub location: RequireLocation,
    pub reexport: bool,
}

struct DestinationPathComponent {
    module_name: Ident,
    doc_comment: String,
}

impl SingleDestination {
    fn path_components(&self) -> Vec<DestinationPathComponent> {
        match self.location {
            RequireLocation::Core { major, minor } => vec![DestinationPathComponent {
                module_name: format_ident!("vk{major}_{minor}"),
                doc_comment: crate::refpage_doc(
                    &format!("VK_VERSION_{major}_{minor}"),
                    format!("Vulkan version {major}.{minor}"),
                ),
            }],
            RequireLocation::Extension { name } => match self.library {
                LibraryName::Vk => {
                    let vulkan_ext = name.strip_prefix("VK_").unwrap();
                    let (ext_tag, ext_name) = vulkan_ext.split_once('_').unwrap();
                    vec![
                        DestinationPathComponent {
                            module_name: format_ident!("{}", ext_tag.to_ascii_lowercase()),
                            doc_comment: format!("Extensions tagged {ext_tag}"),
                        },
                        DestinationPathComponent {
                            module_name: crate::escape_ident(&ext_name.to_snek_case()),
                            doc_comment: crate::refpage_doc(name, format!("Extension `{name}`")),
                        },
                    ]
                }
                LibraryName::Video => {
                    let video_ext = name.strip_prefix("vulkan_video_").unwrap();
                    vec![
                        DestinationPathComponent {
                            module_name: format_ident!("video"),
                            doc_comment: "Vulkan Video".into(),
                        },
                        DestinationPathComponent {
                            module_name: format_ident!("{video_ext}"),
                            doc_comment: format!("Items provided by `{}`", name),
                        },
                    ]
                }
            },
        }
    }

    pub fn doc_link(&self) -> String {
        let components = (self.path_components().iter())
            .map(|component| component.module_name.to_string())
            .join("::");
        format!("Provided by [`{components}`](crate::{components})")
    }
}

#[derive(Default)]
pub struct CodeMap(IndexMap<SingleDestination, TokenStream>);

impl CodeMap {
    pub fn new(dest: SingleDestination, tokens: TokenStream) -> Self {
        let mut map = IndexMap::with_capacity(1);
        map.insert(dest, tokens);
        CodeMap(map)
    }

    pub fn new_from_primary(dest: Destination, tokens: TokenStream) -> Self {
        Self::new(dest.guess_primary(), tokens)
    }

    pub fn extend(&mut self, other: Self) {
        for (destination, tokens) in other.0 {
            self.0.entry(destination).or_default().extend(tokens);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SingleDestination, &TokenStream)> {
        self.0.iter()
    }

    pub fn write(&self, output_path: impl AsRef<Path>) -> io::Result<()> {
        let mut vfs = VirtualRustFs::default();
        vfs.write(
            "mod.rs",
            quote! {
                /// A re-export of all items
                pub mod vk;
            },
        );

        struct ModFile {
            doc_comment: Option<String>,
            child_modules: IndexSet<Ident>,
        }

        struct SourceFile {
            destination: SingleDestination,
            doc_comment: String,
            reexport_content: TokenStream,
            content: TokenStream,
        }

        let mut mod_files: IndexMap<PathBuf, ModFile> = Default::default();
        let mut source_files: IndexMap<PathBuf, SourceFile> = Default::default();
        for (destination, content) in self.iter() {
            let components = destination.path_components();
            if components.is_empty() {
                vfs.write("mod.rs", content.clone());
                continue;
            }

            let doc = &components.last().unwrap().doc_comment;
            let mut path = PathBuf::from_iter(
                (components.iter())
                    .map(|component| PathBuf::from(component.module_name.to_string())),
            );
            path.add_extension("rs");

            let source_file = source_files.entry(path).or_insert_with(|| SourceFile {
                destination: *destination,
                doc_comment: doc.into(),
                reexport_content: TokenStream::new(),
                content: TokenStream::new(),
            });

            if destination.reexport {
                source_file.reexport_content.extend(content.clone());
            } else {
                source_file.content.extend(content.clone());
            }

            // this affects the order impl blocks show up in rustdoc
            let sort_order = match destination.location {
                RequireLocation::Core { .. } => 1,
                RequireLocation::Extension { .. } => 2,
            };

            // collect mod.rs files to be created
            for (i, component) in components.iter().enumerate() {
                let parent_path = &components[0..i];
                let mod_path = PathBuf::from_iter(
                    (parent_path.iter())
                        .map(|component| PathBuf::from(&component.module_name.to_string()))
                        .chain(iter::once(PathBuf::from("mod.rs"))),
                );

                let mod_file = mod_files.entry(mod_path).or_insert_with(|| ModFile {
                    doc_comment: (parent_path.last())
                        .map(|component| component.doc_comment.clone()),
                    child_modules: IndexSet::new(),
                });

                (mod_file.child_modules)
                    .insert_sorted_by_key(component.module_name.clone(), |_| sort_order);
            }
        }

        for (source_path, source_file) in source_files {
            let doc = source_file.doc_comment;
            let content = source_file.content;
            let mut reexport_content = source_file.reexport_content;
            if !reexport_content.is_empty() {
                let mut module = None;
                if !content.is_empty() {
                    module = Some(quote! { ::reexport });
                    reexport_content = quote! {
                        pub(crate) mod reexport { #reexport_content }
                        pub use reexport::*;
                    };
                }

                let components = source_file.destination.path_components();
                let component_idents = components.iter().map(|component| &component.module_name);
                vfs.write(
                    "vk.rs",
                    quote! { pub use super:: #( #component_idents ) :: * #module ::*; },
                );
            }

            vfs.write(
                source_path,
                quote! {
                    #![doc = #doc]
                    #content
                    #reexport_content
                },
            );
        }

        for (mod_path, mod_file) in mod_files {
            let doc = mod_file.doc_comment.map(|doc| quote! { #![doc = #doc] });
            let mod_child_idents = mod_file.child_modules.iter();
            vfs.write(
                mod_path,
                quote! {
                    #doc
                    #(pub mod #mod_child_idents;)*
                },
            );
        }

        vfs.write("vk.rs", quote! { 
            pub use crate::Handle; 
            pub use crate::TaggedStructure;
            pub use crate::Extends;
            pub use crate::platform_types::*;
        });
        vfs.sync_to(output_path)
    }
}
