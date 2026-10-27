//! API de aplicación: adapta las operaciones del núcleo a las necesidades de las interfaces.
//!
//! Coordina operaciones de dominio y almacenamiento; no gestiona procesos del sistema operativo
//! ni duplica reglas semánticas que pertenecen al modelo.

mod application;
mod mask_operations;
mod model_operations;
mod navigation;

pub use application::{
    create_instance_at_current_location, create_profile, create_project, list_projects,
    prepare_workspace, remove_instance_and_update_navigation, ApplicationError, PreparedWorkspace,
    ProjectApplicationError,
};
pub use mask_operations::{
    create_snapshot, edit_instance_component, edit_instance_concept, edit_shared_component,
    edit_shared_concept, remove_mask, resolve_instance, MaskOperationError,
};
pub use model_operations::children_of;
pub use navigation::{NavigationError, NavigationState, NavigationStore};
