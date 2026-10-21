use crate::semantic_masks::{Component, Segment, SegmentRole};
use crate::{load_workspace, LifecycleError, ResourceStore, StoreError, WorkspaceStore};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// Identidad estable del recurso.
    /// Se serializa primero para facilitar la comprobación del archivo.
    pub id: Uuid,
    pub info: Info,
    /// Segmentos semánticos adicionales; ausente en recursos históricos equivale a vacío.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
}

impl Resource {
    /// Proyecta la metadata histórica al segmento semántico reservado info.
    /// El segmento no se duplica en el archivo TOML: se deriva de los campos existentes.
    pub fn semantic_definition(&self) -> crate::semantic_masks::ResourceDefinition {
        let mut segments = vec![Segment {
            role: SegmentRole::Semantic,
            key: "info".to_owned(),
            concepts: vec![
                crate::semantic_masks::Concept {
                    key: "title".to_owned(),
                    value: self.info.title.clone(),
                },
                crate::semantic_masks::Concept {
                    key: "description".to_owned(),
                    value: self.info.description.clone().unwrap_or_default(),
                },
            ],
            components: vec![crate::semantic_masks::Component {
                key: "tags".to_owned(),
                kind: "string-list".to_owned(),
                value: serde_json::Value::Array(
                    self.info
                        .tags
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            }],
        }];
        segments.extend(self.segments.clone());
        crate::semantic_masks::ResourceDefinition {
            id: self.id,
            segments,
        }
    }
}

/// Estado administrativo del recurso. Su contenido no forma parte de la API pública.

/// Información descriptiva del recurso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub title: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourcePatch {
    /// None conserva el título; Some(value) solicita cambiarlo.
    pub title: Option<String>,

    /// None conserva la descripción; Some(None) la elimina.
    /// Some(Some("")) también elimina la descripción.
    pub description: Option<Option<String>>,

    /// None conserva las etiquetas; Some(tags) reemplaza la lista.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceWarning {
    LeadingSpaces { id: Uuid },
}

pub(crate) fn normalize_name(name: &str) -> Result<(String, Vec<ResourceWarning>), ResourceError> {
    let normalized = name.trim_end_matches(' ').to_owned();

    if normalized.is_empty() {
        return Err(ResourceError::InvalidName);
    }

    // Los espacios iniciales se conservan por ahora.
    Ok((normalized, Vec::new()))
}

pub(crate) fn validate_stored_name(name: &str) -> Result<(), ResourceError> {
    if name.is_empty() || name.trim_matches(' ').is_empty() {
        return Err(ResourceError::InvalidName);
    }

    Ok(())
}

pub(crate) fn normalize_description(description: Option<String>) -> Option<String> {
    description.filter(|value| !value.is_empty())
}

/// Error produced by resource-domain validation, independent of persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceError {
    InvalidName,
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => write!(
                f,
                "el nombre no puede estar vacío ni contener solo espacios"
            ),
        }
    }
}

impl std::error::Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation_reports_domain_error_without_storage_types() {
        assert_eq!(
            normalize_name("   ").unwrap_err(),
            ResourceError::InvalidName
        );
        assert_eq!(
            validate_stored_name("   ").unwrap_err(),
            ResourceError::InvalidName
        );
        assert!(normalize_name("valid name").is_ok());
    }
}

const REFERENCE_SEGMENT: &str = "__reference";

/// Errores al crear o interpretar recursos de referencia.
#[derive(Debug)]
pub enum ReferenceError {
    Resource(StoreError),
    Invalid(String),
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => write!(f, "error de recurso: {error}"),
            Self::Invalid(message) => write!(f, "referencia inválida: {message}"),
        }
    }
}

impl std::error::Error for ReferenceError {}

impl From<StoreError> for ReferenceError {
    fn from(value: StoreError) -> Self {
        Self::Resource(value)
    }
}

/// Crea una referencia no instanciable desde una instancia propietaria hacia otra.
/// La identidad del recurso de referencia se registra en semantic.references del propietario.
pub fn create_reference(
    resources: &ResourceStore,
    workspaces: &WorkspaceStore,
    workspace_name: &str,
    owner_instance_id: Uuid,
    target_instance_id: Uuid,
    title: &str,
    description: Option<&str>,
) -> Result<Resource, ReferenceError> {
    let mut workspace = load_workspace(resources, workspaces, workspace_name)
        .map_err(|error: LifecycleError| ReferenceError::Invalid(error.to_string()))?;
    if workspace.find_instance(owner_instance_id).is_none() {
        return Err(ReferenceError::Invalid(format!(
            "no existe la instancia propietaria {owner_instance_id}"
        )));
    }
    if workspace.find_instance(target_instance_id).is_none() {
        return Err(ReferenceError::Invalid(format!(
            "no existe la instancia destino {target_instance_id}"
        )));
    }

    let (resource, _) = resources.create(title, description)?;
    let reference = resources.update_segments(
        resource.id,
        vec![Segment {
            key: REFERENCE_SEGMENT.to_owned(),
            role: SegmentRole::Control,
            concepts: Vec::new(),
            components: vec![Component {
                key: "point".to_owned(),
                kind: "instance-id".to_owned(),
                value: serde_json::Value::String(target_instance_id.to_string()),
            }],
        }],
    )?;

    let owner = workspace
        .find_instance_mut(owner_instance_id)
        .expect("owner was validated above");
    let semantic = owner.semantic.get_or_insert_with(Default::default);
    semantic.references.push(reference.id);
    workspaces
        .save(workspace_name, &workspace)
        .map_err(|error| ReferenceError::Invalid(error.to_string()))?;

    Ok(reference)
}

/// Detecta un recurso de referencia por su segmento de control reservado.
pub fn is_reference(resource: &Resource) -> bool {
    resource
        .segments
        .iter()
        .any(|segment| segment.key == REFERENCE_SEGMENT && segment.role == SegmentRole::Control)
}

/// Lee el UUID de la instancia apuntada por una referencia.
pub fn reference_target(resource: &Resource) -> Result<Uuid, ReferenceError> {
    let segment = resource
        .segments
        .iter()
        .find(|segment| segment.key == REFERENCE_SEGMENT && segment.role == SegmentRole::Control)
        .ok_or_else(|| ReferenceError::Invalid("el recurso no es una referencia".to_owned()))?;
    let point = segment
        .components
        .iter()
        .find(|component| component.key == "point")
        .ok_or_else(|| {
            ReferenceError::Invalid("falta el componente reservado 'point'".to_owned())
        })?;
    if point.kind != "instance-id" {
        return Err(ReferenceError::Invalid(
            "el componente 'point' debe ser de tipo 'instance-id'".to_owned(),
        ));
    }
    let value = point.value.as_str().ok_or_else(|| {
        ReferenceError::Invalid("el componente 'point' debe ser texto".to_owned())
    })?;
    Uuid::parse_str(value)
        .map_err(|_| ReferenceError::Invalid(format!("UUID de instancia inválido: {value}")))
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::{create_instance, ResourceStore, WorkspaceStore};

    #[test]
    fn reference_points_to_an_instance_and_is_registered_without_instantiation() {
        let directory = tempfile::tempdir().unwrap();
        let resources = ResourceStore::new(directory.path().join("resources")).unwrap();
        let workspaces = WorkspaceStore::new(directory.path().join("workspaces")).unwrap();
        let (owner_resource, _) = resources.create("Propietario", None).unwrap();
        let (target_resource, _) = resources.create("Destino", None).unwrap();
        let owner =
            create_instance(&resources, &workspaces, "principal", owner_resource.id).unwrap();
        let target =
            create_instance(&resources, &workspaces, "principal", target_resource.id).unwrap();

        let reference = create_reference(
            &resources,
            &workspaces,
            "principal",
            owner,
            target,
            "Apunta a destino",
            None,
        )
        .unwrap();

        assert!(is_reference(&reference));
        assert_eq!(reference_target(&reference).unwrap(), target);
        let workspace = workspaces.load("principal").unwrap();
        assert_eq!(
            workspace
                .find_instance(owner)
                .unwrap()
                .semantic
                .as_ref()
                .unwrap()
                .references,
            vec![reference.id]
        );
        assert!(matches!(
            crate::create_instance(&resources, &workspaces, "principal", reference.id),
            Err(crate::LifecycleError::ReferenceCannotBeInstantiated(id)) if id == reference.id
        ));
    }
}
