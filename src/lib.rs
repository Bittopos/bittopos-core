//! Núcleo de dominio y almacenamiento de Bittopos.
//!
//! Los recursos se guardan en archivos TOML individuales. La estructura se guarda
//! en un archivo JSON y contiene instancias con identidad propia. La API de aplicación
//! que conecta las interfaces con este núcleo vive en `api`.

pub mod api;
mod database;
mod lifecycle;
mod profile;
mod project;
mod relations;
mod resource;
mod semantic_masks;
mod store;
mod workspace;
mod workspace_model;

pub use database::{Database, DatabaseError};
pub use lifecycle::{
    create_instance, create_instance_under, load_workspace, remove_instance, LifecycleError,
};
pub use profile::{Profile, ProfileError, ProfileStore};
pub use project::{Project, ProjectError, ProjectStore};
pub use relations::{
    create_relation, is_relation, relation_endpoints, RelationEndpoints, RelationError,
};
pub use resource::{
    create_reference, is_reference, reference_target, Info, ReferenceError, Resource,
    ResourcePatch, ResourceWarning,
};
pub use semantic_masks::{
    resolve as resolve_semantics, Component, ComponentMerge, Concept, ElementAddress,
    InstanceSemantics, Mask, MaskError, MaskMode, ReplaceComponent, ResourceDefinition,
    Segment as SemanticSegment, SegmentRole,
};
pub use store::{ResourceStore, StoreError};
pub use workspace::{WorkspaceError, WorkspaceStore};
pub use workspace_model::{Content, Instance, Semantic, Source, Workspace, WorkspaceModelError};
