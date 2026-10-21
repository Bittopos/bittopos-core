use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::resource::{
    normalize_description, normalize_name, validate_stored_name, Info, Resource, ResourceError,
    ResourcePatch, ResourceWarning,
};
use crate::semantic_masks::{ResourceDefinition, Segment, SegmentRole};

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    TomlSerialize(toml::ser::Error),
    TomlParse(toml::de::Error),
    InvalidName,
    NotFound(Uuid),
    InvalidFileName(PathBuf),
    IdMismatch { expected: Uuid, found: Uuid },
    IdNotFirstLine(PathBuf),
    InvalidSemanticModel(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "error de E/S: {e}"),
            Self::TomlSerialize(e) => write!(f, "no se pudo serializar TOML: {e}"),
            Self::TomlParse(e) => write!(f, "TOML inválido: {e}"),
            Self::InvalidName => write!(
                f,
                "el nombre no puede estar vacío ni contener solo espacios"
            ),
            Self::NotFound(id) => write!(f, "no existe el recurso {id}"),
            Self::InvalidFileName(path) => write!(
                f,
                "nombre de archivo de recurso inválido: {}",
                path.display()
            ),
            Self::IdMismatch { expected, found } => write!(
                f,
                "el ID del archivo ({expected}) no coincide con el ID del contenido ({found})"
            ),
            Self::IdNotFirstLine(path) => write!(
                f,
                "el ID no aparece en la primera línea de {}",
                path.display()
            ),
            Self::InvalidSemanticModel(message) => {
                write!(f, "modelo semántico inválido: {message}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<ResourceError> for StoreError {
    fn from(value: ResourceError) -> Self {
        match value {
            ResourceError::InvalidName => Self::InvalidName,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<toml::ser::Error> for StoreError {
    fn from(value: toml::ser::Error) -> Self {
        Self::TomlSerialize(value)
    }
}
impl From<toml::de::Error> for StoreError {
    fn from(value: toml::de::Error) -> Self {
        Self::TomlParse(value)
    }
}

/// Almacén local de recursos. El prototipo presupone un solo proceso escritor.
pub struct ResourceStore {
    root: PathBuf,
}

impl ResourceStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<(Resource, Vec<ResourceWarning>), StoreError> {
        let (name, _) = normalize_name(name).map_err(StoreError::from)?;
        let mut warnings = Vec::new();
        let id = Uuid::now_v7();
        let description = normalize_description(description.map(str::to_owned));
        let resource = Resource {
            id,
            info: Info {
                title: name,
                description,
                tags: Vec::new(),
            },
            segments: Vec::new(),
        };
        self.write_new(&resource)?;
        if resource.info.title.starts_with(' ') {
            warnings.push(ResourceWarning::LeadingSpaces { id });
        }
        Ok((resource, warnings))
    }

    pub fn get(&self, id: Uuid) -> Result<Resource, StoreError> {
        let path = self.path_for(id);
        if !path.is_file() {
            return Err(StoreError::NotFound(id));
        }
        self.read_file(&path, id)
    }

    pub fn update(
        &self,
        id: Uuid,
        patch: ResourcePatch,
    ) -> Result<(Resource, Vec<ResourceWarning>), StoreError> {
        let mut resource = self.get(id)?;
        let mut warnings = Vec::new();

        if let Some(title) = patch.title {
            let (normalized, _) = normalize_name(&title)?;
            if normalized.starts_with(' ') {
                warnings.push(ResourceWarning::LeadingSpaces { id });
            }
            resource.info.title = normalized;
        }

        if let Some(description) = patch.description {
            resource.info.description = normalize_description(description);
        }

        if let Some(tags) = patch.tags {
            resource.info.tags = tags;
        }

        self.write_existing(&resource)?;
        Ok((resource, warnings))
    }

    /// Reemplaza los segmentos semánticos adicionales del recurso.
    /// El segmento reservado info se deriva de Resource.info y no se persiste dos veces.
    pub fn update_segments(
        &self,
        id: Uuid,
        segments: Vec<Segment>,
    ) -> Result<Resource, StoreError> {
        let mut resource = self.get(id)?;
        let is_relation = resource
            .segments
            .iter()
            .any(|segment| segment.key == "__relation" && segment.role == SegmentRole::Control);
        if is_relation
            && !segments
                .iter()
                .any(|segment| segment.key == "__relation" && segment.role == SegmentRole::Control)
        {
            return Err(StoreError::InvalidSemanticModel(
                "no se puede retirar el segmento de control de una relación".to_owned(),
            ));
        }
        resource.segments = segments;
        resource
            .semantic_definition()
            .validate()
            .map_err(|error| StoreError::InvalidSemanticModel(error.to_string()))?;
        self.write_existing(&resource)?;
        Ok(resource)
    }

    /// Persiste una definición semántica resuelta sobre el recurso existente.
    /// Solo admite el esquema conocido del segmento info; los demás segmentos se conservan.
    pub fn update_semantic_definition(
        &self,
        id: Uuid,
        definition: ResourceDefinition,
    ) -> Result<Resource, StoreError> {
        if definition.id != id {
            return Err(StoreError::InvalidSemanticModel(format!(
                "la definición pertenece a {}, no a {id}",
                definition.id
            )));
        }
        definition
            .validate()
            .map_err(|error| StoreError::InvalidSemanticModel(error.to_string()))?;
        let info = definition.segment("info").ok_or_else(|| {
            StoreError::InvalidSemanticModel("falta el segmento reservado info".to_owned())
        })?;
        if info.role != SegmentRole::Semantic {
            return Err(StoreError::InvalidSemanticModel(
                "el segmento reservado info debe tener rol semantic".to_owned(),
            ));
        }
        if info.concepts.len() != 2 || info.components.len() != 1 {
            return Err(StoreError::InvalidSemanticModel(
                "info debe contener title, description y el componente tags".to_owned(),
            ));
        }
        let title = info
            .concepts
            .iter()
            .find(|item| item.key == "title")
            .ok_or_else(|| StoreError::InvalidSemanticModel("falta info.title".to_owned()))?
            .value
            .clone();
        let description = info
            .concepts
            .iter()
            .find(|item| item.key == "description")
            .ok_or_else(|| StoreError::InvalidSemanticModel("falta info.description".to_owned()))?
            .value
            .clone();
        let tags_component = info
            .components
            .iter()
            .find(|item| item.key == "tags")
            .ok_or_else(|| StoreError::InvalidSemanticModel("falta info.tags".to_owned()))?;
        if tags_component.kind != "string-list" {
            return Err(StoreError::InvalidSemanticModel(
                "info.tags debe usar el tipo string-list".to_owned(),
            ));
        }
        let tags = match &tags_component.value {
            serde_json::Value::Array(values) => values
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        StoreError::InvalidSemanticModel("info.tags solo admite cadenas".to_owned())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(StoreError::InvalidSemanticModel(
                    "info.tags debe ser una lista".to_owned(),
                ))
            }
        };
        let mut resource = self.get(id)?;
        let is_relation = resource
            .segments
            .iter()
            .any(|segment| segment.key == "__relation" && segment.role == SegmentRole::Control);
        if is_relation
            && !definition
                .segments
                .iter()
                .any(|segment| segment.key == "__relation" && segment.role == SegmentRole::Control)
        {
            return Err(StoreError::InvalidSemanticModel(
                "no se puede retirar el segmento de control de una relación".to_owned(),
            ));
        }
        let (title, _) = normalize_name(&title)?;
        resource.info.title = title;
        resource.info.description = normalize_description(Some(description));
        resource.info.tags = tags;
        resource.segments = definition
            .segments
            .into_iter()
            .filter(|segment| segment.key != "info")
            .collect();
        self.write_existing(&resource)?;
        Ok(resource)
    }

    fn path_for(&self, id: Uuid) -> PathBuf {
        self.root.join(format!("{id}.toml"))
    }

    fn write_new(&self, resource: &Resource) -> Result<(), StoreError> {
        let path = self.path_for(resource.id);
        if path.exists() {
            return Err(StoreError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("ya existe el recurso {}", path.display()),
            )));
        }
        fs::write(path, toml::to_string(resource)?)?;
        Ok(())
    }

    fn write_existing(&self, resource: &Resource) -> Result<(), StoreError> {
        fs::write(self.path_for(resource.id), toml::to_string(resource)?)?;
        Ok(())
    }

    fn read_file(&self, path: &Path, expected_id: Uuid) -> Result<Resource, StoreError> {
        let content = fs::read_to_string(path)?;
        let first_line = content.lines().next().unwrap_or_default();
        if !first_line.starts_with("id = ") {
            return Err(StoreError::IdNotFirstLine(path.to_path_buf()));
        }
        let resource: Resource = toml::from_str(&content)?;
        if resource.id != expected_id {
            return Err(StoreError::IdMismatch {
                expected: expected_id,
                found: resource.id,
            });
        }
        validate_stored_name(&resource.info.title).map_err(StoreError::from)?;
        Ok(resource)
    }
}

/// Promueve un temporal validado al destino definitivo.
/// En caso de error no elimina el temporal: el llamador puede conservarlo para inspección.
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn store() -> (tempfile::TempDir, ResourceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn creates_uuid_v7_resource_and_toml_file() {
        let (_dir, store) = store();
        let (resource, warnings) = store.create("Primer recurso", Some("Contexto")).unwrap();
        assert_eq!(resource.id.get_version_num(), 7);
        assert!(warnings.is_empty());
        let file = fs::read_to_string(store.path_for(resource.id)).unwrap();
        assert!(file.lines().next().unwrap().starts_with("id = "));
        assert!(file.contains("title = \"Primer recurso\""));
    }

    #[test]
    fn same_name_gets_distinct_ids() {
        let (_dir, store) = store();
        let (a, _) = store.create("Igual", None).unwrap();
        let (b, _) = store.create("Igual", None).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn patch_preserves_omitted_fields_and_identity() {
        let (_dir, store) = store();
        let (created, _) = store.create("Antes", Some("Descripción")).unwrap();
        let (updated, _) = store
            .update(
                created.id,
                ResourcePatch {
                    title: Some("Después  ".to_owned()),
                    description: None,
                    tags: None,
                },
            )
            .unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.info.title, "Después");
        assert_eq!(updated.info.description.as_deref(), Some("Descripción"));
    }

    #[test]
    fn rejects_blank_names_and_warns_about_leading_space() {
        let (_dir, store) = store();
        assert!(matches!(
            store.create("   ", None),
            Err(StoreError::InvalidName)
        ));
        let (resource, warnings) = store.create(" nombre", None).unwrap();
        assert_eq!(resource.info.title, " nombre");
        assert_eq!(
            warnings,
            vec![ResourceWarning::LeadingSpaces { id: resource.id }]
        );
    }

    #[test]
    fn missing_id_returns_controlled_error() {
        let (_dir, store) = store();
        let missing = Uuid::now_v7();
        assert!(matches!(store.get(missing), Err(StoreError::NotFound(id)) if id == missing));
    }

    #[test]
    fn empty_description_clears_it() {
        let (_dir, store) = store();
        let (resource, _) = store.create("Recurso", Some("Texto")).unwrap();
        let (updated, _) = store
            .update(
                resource.id,
                ResourcePatch {
                    title: None,
                    description: Some(Some(String::new())),
                    tags: None,
                },
            )
            .unwrap();
        assert_eq!(updated.info.description, None);
    }
}
