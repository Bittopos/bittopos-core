use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ficha de proyecto, vinculada a un único perfil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug)]
pub enum ProjectError {
    Io(io::Error),
    Json(serde_json::Error),
    TomlSerialize(toml::ser::Error),
    TomlParse(toml::de::Error),
    InvalidName,
    NotFound(Uuid),
    ProfileMismatch { expected: Uuid, found: Uuid },
    MetadataMismatch(Uuid),
    InvalidDirectory(PathBuf),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "error de E/S del proyecto: {error}"),
            Self::Json(error) => write!(f, "JSON de proyecto inválido: {error}"),
            Self::TomlSerialize(error) => {
                write!(f, "no se pudo serializar TOML del proyecto: {error}")
            }
            Self::TomlParse(error) => write!(f, "TOML de proyecto inválido: {error}"),
            Self::InvalidName => write!(f, "el nombre del proyecto no puede estar vacío"),
            Self::NotFound(id) => write!(f, "no existe el proyecto {id}"),
            Self::ProfileMismatch { expected, found } => {
                write!(
                    f,
                    "el proyecto pertenece al perfil {found}, no al perfil {expected}"
                )
            }
            Self::MetadataMismatch(id) => {
                write!(
                    f,
                    "los metadatos JSON y TOML del proyecto {id} no coinciden"
                )
            }
            Self::InvalidDirectory(path) => {
                write!(
                    f,
                    "directorio de proyecto no identificado por UUID: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<io::Error> for ProjectError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for ProjectError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl From<toml::ser::Error> for ProjectError {
    fn from(error: toml::ser::Error) -> Self {
        Self::TomlSerialize(error)
    }
}
impl From<toml::de::Error> for ProjectError {
    fn from(error: toml::de::Error) -> Self {
        Self::TomlParse(error)
    }
}

/// Almacena proyectos dentro del directorio que contiene profile.json.
pub struct ProjectStore {
    root: PathBuf,
}

impl ProjectStore {
    pub fn new(profile_dir: impl AsRef<Path>) -> Self {
        Self {
            root: profile_dir.as_ref().join("projects"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(
        &self,
        profile_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Project, ProjectError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ProjectError::InvalidName);
        }
        let description = description
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        fs::create_dir_all(&self.root)?;

        let id = Uuid::now_v7();
        let project_dir = self.root.join(id.to_string());
        fs::create_dir(&project_dir)?;

        let project = Project {
            id,
            profile_id,
            name: name.to_owned(),
            description,
        };
        let result = (|| {
            fs::create_dir(project_dir.join("resources"))?;
            fs::create_dir(project_dir.join("workspaces"))?;
            let json = serde_json::to_vec_pretty(&project)?;
            let toml = toml::to_string_pretty(&project)?;
            let mut json_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(project_dir.join("project.json"))?;
            json_file.write_all(&json)?;
            json_file.write_all(b"\n")?;
            json_file.sync_all()?;
            let mut toml_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(project_dir.join("project.toml"))?;
            toml_file.write_all(toml.as_bytes())?;
            toml_file.write_all(b"\n")?;
            toml_file.sync_all()?;
            Ok::<(), ProjectError>(())
        })();

        if let Err(error) = result {
            let _ = fs::remove_dir_all(&project_dir);
            return Err(error);
        }
        Ok(project)
    }

    pub fn load(&self, profile_id: Uuid, id: Uuid) -> Result<Project, ProjectError> {
        let project_dir = self.root.join(id.to_string());
        let json = match fs::read(project_dir.join("project.json")) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ProjectError::NotFound(id))
            }
            Err(error) => return Err(ProjectError::Io(error)),
        };
        let toml = fs::read_to_string(project_dir.join("project.toml"))?;
        let from_json: Project = serde_json::from_slice(&json)?;
        let from_toml: Project = toml::from_str(&toml)?;
        if from_json != from_toml || from_json.id != id {
            return Err(ProjectError::MetadataMismatch(id));
        }
        if from_json.profile_id != profile_id {
            return Err(ProjectError::ProfileMismatch {
                expected: profile_id,
                found: from_json.profile_id,
            });
        }
        Ok(from_json)
    }

    pub fn list(&self, profile_id: Uuid) -> Result<Vec<Project>, ProjectError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ProjectError::Io(error)),
        };
        let mut projects = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| ProjectError::InvalidDirectory(path.clone()))?;
            let id =
                Uuid::parse_str(name).map_err(|_| ProjectError::InvalidDirectory(path.clone()))?;
            projects.push(self.load(profile_id, id)?);
        }
        projects.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(projects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn creates_project_folder_with_json_toml_and_project_scoped_storage() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let profile_id = Uuid::now_v7();

        let project = store
            .create(profile_id, "  Estudio  ", Some("  Notas  "))
            .unwrap();
        let project_dir = store.root().join(project.id.to_string());
        let json: Value =
            serde_json::from_slice(&fs::read(project_dir.join("project.json")).unwrap()).unwrap();
        let toml: Project =
            toml::from_str(&fs::read_to_string(project_dir.join("project.toml")).unwrap()).unwrap();

        assert_eq!(project.name, "Estudio");
        assert_eq!(project.description.as_deref(), Some("Notas"));
        assert_eq!(toml, project);
        assert_eq!(json["id"], project.id.to_string());
        assert_eq!(json["profile_id"], profile_id.to_string());
        assert!(project_dir.join("resources").is_dir());
        assert!(project_dir.join("workspaces").is_dir());
    }

    #[test]
    fn lists_only_projects_owned_by_the_requested_profile() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let profile_id = Uuid::now_v7();
        let other_profile = Uuid::now_v7();
        let project = store.create(profile_id, "Uno", None).unwrap();

        assert_eq!(store.list(profile_id).unwrap(), vec![project.clone()]);
        assert!(matches!(
            store.load(other_profile, project.id),
            Err(ProjectError::ProfileMismatch { .. })
        ));
    }

    #[test]
    fn rejects_empty_project_names_and_lists_empty_profile() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        assert!(matches!(
            store.create(Uuid::now_v7(), "  ", None),
            Err(ProjectError::InvalidName)
        ));
        assert!(store.list(Uuid::now_v7()).unwrap().is_empty());
    }

    #[test]
    fn detects_mismatched_json_and_toml() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let profile_id = Uuid::now_v7();
        let project = store.create(profile_id, "Uno", None).unwrap();
        let project_dir = store.root().join(project.id.to_string());
        fs::write(
            project_dir.join("project.toml"),
            "id = \"00000000-0000-7000-8000-000000000000\"\nprofile_id = \"00000000-0000-7000-8000-000000000000\"\nname = \"distinto\"\n",
        )
        .unwrap();
        assert!(matches!(
            store.load(profile_id, project.id),
            Err(ProjectError::MetadataMismatch(_))
        ));
    }
}
