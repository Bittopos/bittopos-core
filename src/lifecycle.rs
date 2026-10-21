//! Operaciones básicas sobre recursos e instancias.
//!
//! La estructura persistida es la fuente de organización de las instancias.
//! Las operaciones escriben directamente los archivos implicados; no hay journal
//! ni reconciliación automática entre archivos.

use std::fmt;
use uuid::Uuid;

use crate::{
    is_reference, is_relation, Instance, ResourceStore, StoreError, Workspace, WorkspaceError,
    WorkspaceStore,
};

#[derive(Debug)]
pub enum LifecycleError {
    Resource(StoreError),
    Workspace(WorkspaceError),
    ParentRequired,
    InstanceNotFound(Uuid),
    RelationCannotBeInstantiated(Uuid),
    ReferenceCannotBeInstantiated(Uuid),
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => write!(f, "error de recurso: {error}"),
            Self::Workspace(error) => write!(f, "error de estructura: {error}"),
            Self::ParentRequired => write!(f, "la instancia padre no existe"),
            Self::InstanceNotFound(id) => write!(f, "no existe la instancia {id}"),
            Self::RelationCannotBeInstantiated(id) => write!(
                f,
                "la relación {id} se lee directamente y no puede instanciarse"
            ),
            Self::ReferenceCannotBeInstantiated(id) => write!(
                f,
                "la referencia {id} se lee directamente y no puede instanciarse"
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}

impl From<StoreError> for LifecycleError {
    fn from(error: StoreError) -> Self {
        Self::Resource(error)
    }
}

impl From<WorkspaceError> for LifecycleError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<crate::WorkspaceModelError> for LifecycleError {
    fn from(error: crate::WorkspaceModelError) -> Self {
        Self::Workspace(WorkspaceError::from(error))
    }
}

/// Carga la estructura. Si aún no existe, comienza vacía.
/// Comprueba que cada instancia referencia un recurso existente.
pub fn load_workspace(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
) -> Result<Workspace, LifecycleError> {
    let workspace = match workspaces.load(workspace_name) {
        Ok(workspace) => workspace,
        Err(WorkspaceError::NotFound(_)) => Workspace::new(),
        Err(error) => return Err(error.into()),
    };

    workspace.validate()?;
    for instance in workspace.all_instances() {
        let resource = resources.get(instance.source.id)?;
        if is_relation(&resource) {
            return Err(LifecycleError::RelationCannotBeInstantiated(resource.id));
        }
        if is_reference(&resource) {
            return Err(LifecycleError::ReferenceCannotBeInstantiated(resource.id));
        }
    }
    Ok(workspace)
}

pub fn create_instance(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    resource_id: Uuid,
) -> Result<Uuid, LifecycleError> {
    create_instance_under(resources, workspaces, workspace_name, resource_id, None)
}

/// Inserta una instancia raíz o hija y persiste la estructura.
/// Las escrituras de recurso y estructura no forman una transacción conjunta.
pub fn create_instance_under(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    resource_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<Uuid, LifecycleError> {
    let resource = resources.get(resource_id)?;
    if is_relation(&resource) {
        return Err(LifecycleError::RelationCannotBeInstantiated(resource_id));
    }
    if is_reference(&resource) {
        return Err(LifecycleError::ReferenceCannotBeInstantiated(resource_id));
    }
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;

    if let Some(parent) = parent_id {
        if workspace.find_instance(parent).is_none() {
            return Err(LifecycleError::ParentRequired);
        }
    }

    let instance = Instance::new(Uuid::now_v7(), resource_id);
    match parent_id {
        Some(parent) => workspace.insert_child(parent, instance.clone())?,
        None => workspace.insert_root(instance.clone())?,
    }
    workspaces.save(workspace_name, &workspace)?;
    Ok(instance.id)
}

/// Elimina una instancia hoja y conserva el recurso compartido.
pub fn remove_instance(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
) -> Result<(), LifecycleError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;
    let instance = workspace
        .find_instance(instance_id)
        .cloned()
        .ok_or(LifecycleError::InstanceNotFound(instance_id))?;

    if !instance.children().is_empty() {
        return Err(WorkspaceError::InstanceHasChildren(instance_id).into());
    }

    workspace.remove_leaf(instance_id)?;
    workspaces.save(workspace_name, &workspace)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, ResourceStore, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let resources = ResourceStore::new(dir.path().join("resources")).unwrap();
        let workspaces = WorkspaceStore::new(dir.path().join("structures")).unwrap();
        (dir, resources, workspaces)
    }

    #[test]
    fn creates_nested_instances_in_the_structure_file() {
        let (_dir, resources, workspaces) = setup();
        let (root_resource, _) = resources.create("Raíz", None).unwrap();
        let (child_resource, _) = resources.create("Hija", None).unwrap();
        let root = create_instance(&resources, &workspaces, "principal", root_resource.id).unwrap();
        let child = create_instance_under(
            &resources,
            &workspaces,
            "principal",
            child_resource.id,
            Some(root),
        )
        .unwrap();

        let structure = workspaces.load("principal").unwrap();
        assert_eq!(structure.parent_of(child), Some(root));
        assert_eq!(
            structure.find_instance(child).unwrap().source.id,
            child_resource.id
        );
    }

    #[test]
    fn removes_leaf_instance_but_keeps_its_resource() {
        let (_dir, resources, workspaces) = setup();
        let (resource, _) = resources.create("Recurso", None).unwrap();
        let id = create_instance(&resources, &workspaces, "principal", resource.id).unwrap();

        remove_instance(&resources, &workspaces, "principal", id).unwrap();

        assert!(workspaces
            .load("principal")
            .unwrap()
            .find_instance(id)
            .is_none());
        assert!(resources.get(resource.id).is_ok());
    }

    #[test]
    fn refuses_to_remove_an_instance_with_children() {
        let (_dir, resources, workspaces) = setup();
        let (resource, _) = resources.create("Recurso", None).unwrap();
        let root = create_instance(&resources, &workspaces, "principal", resource.id).unwrap();
        create_instance_under(
            &resources,
            &workspaces,
            "principal",
            resource.id,
            Some(root),
        )
        .unwrap();

        assert!(remove_instance(&resources, &workspaces, "principal", root).is_err());
    }

    #[test]
    fn refuses_to_instantiate_a_relation_resource() {
        let (_dir, resources, workspaces) = setup();
        let (source, _) = resources.create("Origen", None).unwrap();
        let (target, _) = resources.create("Destino", None).unwrap();
        let relation = crate::create_relation(
            &resources,
            &workspaces,
            "principal",
            "Relación",
            None,
            crate::RelationEndpoints::Resources {
                source: source.id,
                target: target.id,
            },
        )
        .unwrap();

        assert!(matches!(
            create_instance(&resources, &workspaces, "principal", relation.id),
            Err(LifecycleError::RelationCannotBeInstantiated(id)) if id == relation.id
        ));
    }
}
