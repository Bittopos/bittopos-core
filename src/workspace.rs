use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::semantic_masks::Mask;
use crate::workspace_model::{Content, Instance, Semantic, Source, Workspace, WorkspaceModelError};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedContent {
    #[serde(default)]
    instances: Vec<PersistedInstance>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedSemantic {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allocation: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    references: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedSource {
    id: Uuid,
}

/// Representación persistida de una instancia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistedInstance {
    pub(crate) id: Uuid,
    source: PersistedSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic: Option<PersistedSemantic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<PersistedContent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    masks: Vec<Mask>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedWorkspace {
    #[serde(default)]
    content: PersistedContent,
}

impl From<&Instance> for PersistedInstance {
    fn from(instance: &Instance) -> Self {
        Self {
            id: instance.id,
            source: PersistedSource {
                id: instance.source.id,
            },
            semantic: instance
                .semantic
                .as_ref()
                .map(|semantic| PersistedSemantic {
                    allocation: semantic.allocation.clone(),
                    references: semantic.references.clone(),
                }),
            content: instance.content.as_ref().map(|content| PersistedContent {
                instances: content
                    .instances
                    .iter()
                    .map(PersistedInstance::from)
                    .collect(),
            }),
            masks: instance.masks.clone(),
        }
    }
}

impl From<Instance> for PersistedInstance {
    fn from(instance: Instance) -> Self {
        Self::from(&instance)
    }
}

impl TryFrom<PersistedInstance> for Instance {
    type Error = WorkspaceModelError;

    fn try_from(persisted: PersistedInstance) -> Result<Self, Self::Error> {
        let content = match persisted.content {
            Some(content) => Some(Content {
                instances: content
                    .instances
                    .into_iter()
                    .map(Instance::try_from)
                    .collect::<Result<_, _>>()?,
            }),
            None => None,
        };

        let instance = Instance {
            id: persisted.id,
            source: Source {
                id: persisted.source.id,
            },
            semantic: persisted.semantic.map(|semantic| Semantic {
                allocation: semantic.allocation,
                references: semantic.references,
            }),
            content,
            masks: persisted.masks,
        };

        Workspace {
            content: Content {
                instances: vec![instance.clone()],
            },
        }
        .validate()?;

        Ok(instance)
    }
}

impl TryFrom<PersistedWorkspace> for Workspace {
    type Error = WorkspaceModelError;

    fn try_from(persisted: PersistedWorkspace) -> Result<Self, Self::Error> {
        let workspace = Workspace {
            content: Content {
                instances: persisted
                    .content
                    .instances
                    .into_iter()
                    .map(Instance::try_from)
                    .collect::<Result<_, _>>()?,
            },
        };

        workspace.validate()?;
        Ok(workspace)
    }
}

impl From<&Workspace> for PersistedWorkspace {
    fn from(workspace: &Workspace) -> Self {
        Self {
            content: PersistedContent {
                instances: workspace
                    .content
                    .instances
                    .iter()
                    .map(PersistedInstance::from)
                    .collect(),
            },
        }
    }
}

#[derive(Debug)]
pub enum WorkspaceError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidName,
    NotFound(String),
    DuplicateInstanceId(Uuid),
    InvalidContainment(String),
    InstanceNotFound(Uuid),
    InstanceHasChildren(Uuid),
    ParentNotFound(Uuid),
    InvalidMask(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "error de E/S: {error}"),
            Self::Json(error) => write!(f, "JSON inválido: {error}"),
            Self::InvalidName => write!(f, "nombre de workspace inválido"),
            Self::NotFound(name) => write!(f, "no existe el workspace {name}"),
            Self::DuplicateInstanceId(id) => {
                write!(f, "el ID de instancia {id} está duplicado en el workspace")
            }
            Self::InvalidContainment(reason) => write!(f, "contención inválida: {reason}"),
            Self::InstanceNotFound(id) => write!(f, "no existe la instancia {id}"),
            Self::InstanceHasChildren(id) => write!(
                f,
                "no se puede remover la instancia {id}: todavía contiene instancias hijas"
            ),
            Self::ParentNotFound(id) => write!(f, "no existe la instancia padre {id}"),
            Self::InvalidMask(reason) => write!(f, "máscara semántica inválida: {reason}"),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl From<WorkspaceModelError> for WorkspaceError {
    fn from(error: WorkspaceModelError) -> Self {
        match error {
            WorkspaceModelError::DuplicateInstanceId(id) => Self::DuplicateInstanceId(id),
            WorkspaceModelError::InvalidContainment(reason) => Self::InvalidContainment(reason),
            WorkspaceModelError::InstanceNotFound(id) => Self::InstanceNotFound(id),
            WorkspaceModelError::InstanceHasChildren(id) => Self::InstanceHasChildren(id),
            WorkspaceModelError::ParentNotFound(id) => Self::ParentNotFound(id),
            WorkspaceModelError::InvalidMask(reason) => Self::InvalidMask(reason),
        }
    }
}

impl From<io::Error> for WorkspaceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for WorkspaceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub struct WorkspaceStore {
    root: PathBuf,
}

impl WorkspaceStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, WorkspaceError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load(&self, name: &str) -> Result<Workspace, WorkspaceError> {
        let path = self.path_for(name)?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(WorkspaceError::NotFound(name.to_owned()));
            }
            Err(error) => return Err(WorkspaceError::Io(error)),
        };

        let persisted: PersistedWorkspace = serde_json::from_str(&content)?;
        Workspace::try_from(persisted).map_err(WorkspaceError::from)
    }

    pub fn save(&self, name: &str, workspace: &Workspace) -> Result<(), WorkspaceError> {
        workspace.validate().map_err(WorkspaceError::from)?;

        let path = self.path_for(name)?;
        let content = serde_json::to_vec_pretty(&PersistedWorkspace::from(workspace))?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>, WorkspaceError> {
        let mut names = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            let is_hidden = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));

            if is_hidden
                || path.extension().and_then(|extension| extension.to_str()) != Some("json")
            {
                continue;
            }

            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                names.push(name.to_owned());
            }
        }

        names.sort();
        Ok(names)
    }

    fn path_for(&self, name: &str) -> Result<PathBuf, WorkspaceError> {
        if name.trim().is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
            || name.contains('\0')
        {
            return Err(WorkspaceError::InvalidName);
        }

        Ok(self.root.join(format!("{name}.json")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Concept, ReplaceComponent, ResourceStore, SegmentRole, SemanticSegment};
    use std::fs;

    fn setup() -> (tempfile::TempDir, ResourceStore, WorkspaceStore) {
        let directory = tempfile::tempdir().unwrap();
        let resources = ResourceStore::new(directory.path().join("resources")).unwrap();
        let workspaces = WorkspaceStore::new(directory.path().join("workspaces")).unwrap();
        (directory, resources, workspaces)
    }

    #[test]
    fn nested_workspace_json_round_trips_without_schema_changes() {
        let (_directory, resources, workspaces) = setup();
        let (root_resource, _) = resources.create("Raíz", None).unwrap();
        let (child_resource, _) = resources.create("Hija", None).unwrap();
        let child_id = Uuid::now_v7();

        let mut root = Instance::new(Uuid::now_v7(), root_resource.id);
        root.content = Some(Content {
            instances: vec![Instance::new(child_id, child_resource.id)],
        });
        root.semantic = Some(Semantic {
            allocation: vec![child_id],
            references: Vec::new(),
        });

        let workspace = Workspace {
            content: Content {
                instances: vec![root],
            },
        };

        workspaces.save("proyecto", &workspace).unwrap();
        assert_eq!(workspaces.load("proyecto").unwrap(), workspace);

        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(workspaces.root().join("proyecto.json")).unwrap())
                .unwrap();

        assert_eq!(
            json["content"]["instances"][0]["semantic"]["allocation"][0],
            child_id.to_string()
        );
    }

    #[test]
    fn rejects_legacy_flat_workspace_without_modifying_file() {
        let (_directory, _resources, workspaces) = setup();
        let path = workspaces.root().join("proyecto.json");
        let legacy = r#"{"instances":[]}"#;
        fs::write(&path, legacy).unwrap();

        assert!(matches!(
            workspaces.load("proyecto"),
            Err(WorkspaceError::Json(_))
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), legacy);
    }

    #[test]
    fn empty_workspace_persists_root_instances_array() {
        let (_directory, _resources, workspaces) = setup();
        workspaces.save("proyecto", &Workspace::new()).unwrap();

        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(workspaces.root().join("proyecto.json")).unwrap())
                .unwrap();

        assert_eq!(json["content"]["instances"], serde_json::json!([]));
    }

    #[test]
    fn semantic_masks_round_trip_in_workspace_json_and_resolve_from_toml_segments() {
        let (_directory, resources, workspaces) = setup();
        let (created, _) = resources.create("Definición", None).unwrap();
        let resource = resources
            .update_segments(
                created.id,
                vec![SemanticSegment {
                    role: SegmentRole::Semantic,
                    key: "details".to_owned(),
                    concepts: vec![Concept {
                        key: "title".to_owned(),
                        value: "Original".to_owned(),
                    }],
                    components: Vec::new(),
                }],
            )
            .unwrap();

        let instance_id = Uuid::now_v7();
        let mut instance = Instance::new(instance_id, resource.id);
        instance
            .edit_local_concept(&resource, "details", "title", "Local")
            .unwrap();
        let workspace = Workspace {
            content: Content {
                instances: vec![instance],
            },
        };
        workspaces.save("masked", &workspace).unwrap();

        let loaded = workspaces.load("masked").unwrap();
        assert_eq!(loaded, workspace);
        let stored_resource = resources.get(resource.id).unwrap();
        let resolved = loaded
            .find_instance(instance_id)
            .unwrap()
            .resolved_semantics(&stored_resource, &ReplaceComponent)
            .unwrap();
        assert_eq!(
            resolved.segment("details").unwrap().concepts[0].value,
            "Local"
        );

        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(workspaces.root().join("masked.json")).unwrap())
                .unwrap();
        assert_eq!(
            json["content"]["instances"][0]["masks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn listing_ignores_hidden_files() {
        let (_directory, _resources, workspaces) = setup();
        fs::write(workspaces.root().join(".hidden.json"), "{}").unwrap();
        fs::write(
            workspaces.root().join("proyecto.json"),
            r#"{"content":{"instances":[]}}"#,
        )
        .unwrap();

        assert_eq!(workspaces.list().unwrap(), vec!["proyecto".to_owned()]);
    }
}
