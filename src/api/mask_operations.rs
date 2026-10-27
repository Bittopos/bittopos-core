//! Casos de uso para aplicar y resolver máscaras sin duplicar reglas de dominio.

use std::fmt;
use uuid::Uuid;

use crate::{
    load_workspace, ComponentMerge, ElementAddress, Instance, LifecycleError, MaskError, MaskMode,
    Resource, ResourceDefinition, ResourceStore, StoreError, WorkspaceError, WorkspaceStore,
};

#[derive(Debug)]
pub enum MaskOperationError {
    Lifecycle(LifecycleError),
    Resource(StoreError),
    Workspace(WorkspaceError),
    Mask(MaskError),
    InstanceNotFound(Uuid),
    SourceMismatch {
        instance_id: Uuid,
        expected: Uuid,
        found: Uuid,
    },
}

impl fmt::Display for MaskOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lifecycle(error) => write!(f, "no se pudo cargar la estructura: {error}"),
            Self::Resource(error) => write!(f, "error de recurso: {error}"),
            Self::Workspace(error) => write!(f, "error de workspace: {error}"),
            Self::Mask(error) => write!(f, "error de máscara: {error}"),
            Self::InstanceNotFound(id) => write!(f, "no existe la instancia {id}"),
            Self::SourceMismatch { instance_id, expected, found } => write!(
                f, "la instancia {instance_id} apunta al recurso {found}, no al recurso esperado {expected}"
            ),
        }
    }
}
impl std::error::Error for MaskOperationError {}
impl From<LifecycleError> for MaskOperationError {
    fn from(e: LifecycleError) -> Self {
        Self::Lifecycle(e)
    }
}
impl From<StoreError> for MaskOperationError {
    fn from(e: StoreError) -> Self {
        Self::Resource(e)
    }
}
impl From<WorkspaceError> for MaskOperationError {
    fn from(e: WorkspaceError) -> Self {
        Self::Workspace(e)
    }
}
impl From<MaskError> for MaskOperationError {
    fn from(e: MaskError) -> Self {
        Self::Mask(e)
    }
}

fn load_instance<'a>(
    workspace: &'a mut crate::Workspace,
    id: Uuid,
) -> Result<&'a mut Instance, MaskOperationError> {
    workspace
        .find_instance_mut(id)
        .ok_or(MaskOperationError::InstanceNotFound(id))
}

/// Edita la definición compartida. Las instancias sin diferencias locales
/// verán el cambio al resolverlas de nuevo.
pub fn edit_shared_concept(
    resources: &ResourceStore,
    resource_id: Uuid,
    segment: &str,
    concept: &str,
    value: impl Into<String>,
) -> Result<Resource, MaskOperationError> {
    let resource = resources.get(resource_id)?;
    let mut definition = resource.semantic_definition();
    definition.edit_shared_concept(segment, concept, value)?;
    Ok(resources.update_semantic_definition(resource_id, definition)?)
}

/// Crea o actualiza una diferencia puntual de concepto en una sola instancia.
/// Edita un componente de la definición compartida y persiste el cambio.
pub fn edit_shared_component(
    resources: &ResourceStore,
    resource_id: Uuid,
    segment: &str,
    component: &str,
    value: serde_json::Value,
) -> Result<Resource, MaskOperationError> {
    let resource = resources.get(resource_id)?;
    let mut definition = resource.semantic_definition();
    definition.edit_shared_component(segment, component, value)?;
    Ok(resources.update_semantic_definition(resource_id, definition)?)
}

pub fn edit_instance_concept(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
    segment: &str,
    concept: &str,
    value: impl Into<String>,
) -> Result<(), MaskOperationError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;
    let instance = load_instance(&mut workspace, instance_id)?;
    let resource = resources.get(instance.source.id)?;
    instance.edit_local_concept(&resource, segment, concept, value)?;
    workspace.validate().map_err(WorkspaceError::from)?;
    workspaces.save(workspace_name, &workspace)?;
    Ok(())
}

/// Crea o actualiza una diferencia puntual de componente.
pub fn edit_instance_component(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
    segment: &str,
    component: &str,
    value: serde_json::Value,
) -> Result<(), MaskOperationError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;
    let instance = load_instance(&mut workspace, instance_id)?;
    let resource = resources.get(instance.source.id)?;
    instance.edit_local_component(&resource, segment, component, value)?;
    workspace.validate().map_err(WorkspaceError::from)?;
    workspaces.save(workspace_name, &workspace)?;
    Ok(())
}

/// Congela únicamente los elementos indicados en el alcance de Snapshot.
pub fn create_snapshot(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
    segment: &str,
    scope: &[ElementAddress],
) -> Result<(), MaskOperationError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;
    let instance = load_instance(&mut workspace, instance_id)?;
    let resource = resources.get(instance.source.id)?;
    instance.create_mask_snapshot(&resource, segment, scope)?;
    workspace.validate().map_err(WorkspaceError::from)?;
    workspaces.save(workspace_name, &workspace)?;
    Ok(())
}

/// Quita una máscara completa; la resolución vuelve a heredar esas partes.
pub fn remove_mask(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
    segment: &str,
    mode: MaskMode,
) -> Result<(), MaskOperationError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)?;
    load_instance(&mut workspace, instance_id)?.remove_semantic_mask(segment, mode)?;
    workspace.validate().map_err(WorkspaceError::from)?;
    workspaces.save(workspace_name, &workspace)?;
    Ok(())
}

/// Devuelve la vista semántica efectiva sin modificar la definición ni el workspace.
pub fn resolve_instance(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    instance_id: Uuid,
    component_merge: &impl ComponentMerge,
) -> Result<ResourceDefinition, MaskOperationError> {
    let workspace = load_workspace(resources, workspaces, workspace_name)?;
    let instance = workspace
        .find_instance(instance_id)
        .ok_or(MaskOperationError::InstanceNotFound(instance_id))?;
    let resource = resources.get(instance.source.id)?;
    Ok(instance.resolved_semantics(&resource, component_merge)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Concept, ReplaceComponent, SegmentRole, SemanticSegment};
    use tempfile::tempdir;

    #[test]
    fn persisted_local_mask_survives_reload_and_shared_edits_flow_through_unmasked_concepts() {
        let directory = tempdir().unwrap();
        let resources = ResourceStore::new(directory.path().join("resources")).unwrap();
        let workspaces = WorkspaceStore::new(directory.path().join("workspaces")).unwrap();
        let (created, _) = resources.create("Resource", None).unwrap();
        resources
            .update_segments(
                created.id,
                vec![SemanticSegment {
                    role: SegmentRole::Semantic,
                    key: "details".to_owned(),
                    concepts: vec![
                        Concept {
                            key: "title".to_owned(),
                            value: "Original".to_owned(),
                        },
                        Concept {
                            key: "description".to_owned(),
                            value: "Description 1".to_owned(),
                        },
                    ],
                    components: Vec::new(),
                }],
            )
            .unwrap();

        let instance_id =
            crate::create_instance(&resources, &workspaces, "workspace", created.id).unwrap();

        edit_instance_concept(
            &resources,
            &workspaces,
            "workspace",
            instance_id,
            "details",
            "title",
            "Local",
        )
        .unwrap();
        edit_shared_concept(
            &resources,
            created.id,
            "details",
            "description",
            "Description 2",
        )
        .unwrap();

        let resolved = resolve_instance(
            &resources,
            &workspaces,
            "workspace",
            instance_id,
            &ReplaceComponent,
        )
        .unwrap();
        let segment = resolved.segment("details").unwrap();
        assert_eq!(segment.concepts[0].value, "Local");
        assert_eq!(segment.concepts[1].value, "Description 2");
    }
    #[test]
    fn snapshot_is_persisted_and_freezes_only_selected_info_concept() {
        let directory = tempdir().unwrap();
        let resources = ResourceStore::new(directory.path().join("resources")).unwrap();
        let workspaces = WorkspaceStore::new(directory.path().join("workspaces")).unwrap();
        let (resource, _) = resources
            .create("Original title", Some("Description 1"))
            .unwrap();
        let instance_id =
            crate::create_instance(&resources, &workspaces, "workspace", resource.id).unwrap();

        create_snapshot(
            &resources,
            &workspaces,
            "workspace",
            instance_id,
            "info",
            &[ElementAddress::Concept("title".to_owned())],
        )
        .unwrap();
        edit_shared_concept(&resources, resource.id, "info", "title", "New title").unwrap();
        edit_shared_concept(
            &resources,
            resource.id,
            "info",
            "description",
            "Description 2",
        )
        .unwrap();

        let resolved = resolve_instance(
            &resources,
            &workspaces,
            "workspace",
            instance_id,
            &ReplaceComponent,
        )
        .unwrap();
        let info = resolved.segment("info").unwrap();
        assert_eq!(
            info.concepts
                .iter()
                .find(|c| c.key == "title")
                .unwrap()
                .value,
            "Original title"
        );
        assert_eq!(
            info.concepts
                .iter()
                .find(|c| c.key == "description")
                .unwrap()
                .value,
            "Description 2"
        );
    }

    #[test]
    fn shared_info_edits_are_written_to_the_legacy_resource_fields() {
        let directory = tempdir().unwrap();
        let resources = ResourceStore::new(directory.path().join("resources")).unwrap();
        let (resource, _) = resources.create("Old title", None).unwrap();
        edit_shared_concept(&resources, resource.id, "info", "title", "New title").unwrap();
        assert_eq!(resources.get(resource.id).unwrap().info.title, "New title");
    }
}
