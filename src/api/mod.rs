//! API de aplicación: adapta las operaciones del núcleo a las necesidades de las interfaces.
//!
//! Esta capa puede coordinar operaciones y mantener estado de interacción persistido,
//! pero no administra procesos del sistema operativo ni duplica reglas semánticas del dominio.

mod application;
mod navigation;

pub use application::{
    create_instance_at_current_location, create_profile, create_project, list_projects,
    prepare_workspace, remove_instance_and_update_navigation, ApplicationError, PreparedWorkspace,
    ProjectApplicationError,
};
pub use navigation::{NavigationError, NavigationState, NavigationStore};
