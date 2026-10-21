use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identidad raíz de un espacio de trabajo. Los proyectos dependerán de este ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: Uuid,
}

#[derive(Debug)]
pub enum ProfileError {
    Io(io::Error),
    Json(serde_json::Error),
    AlreadyExists,
    NotFound,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "error de E/S del perfil: {error}"),
            Self::Json(error) => write!(f, "JSON de perfil inválido: {error}"),
            Self::AlreadyExists => write!(f, "el espacio de trabajo ya tiene un perfil"),
            Self::NotFound => write!(f, "el espacio de trabajo no tiene perfil"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<io::Error> for ProfileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ProfileError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Acceso al único archivo profile.json de un espacio de trabajo.
pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new(workspace_dir: impl AsRef<Path>) -> Self {
        Self {
            path: workspace_dir.as_ref().join("profile.json"),
        }
    }

    /// Crea el perfil raíz una sola vez, con un UUID nuevo y sin proyectos asociados.
    pub fn create(&self) -> Result<Profile, ProfileError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let profile = Profile { id: Uuid::now_v7() };
        let contents = serde_json::to_vec_pretty(&profile)?;
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(ProfileError::AlreadyExists);
            }
            Err(error) => return Err(ProfileError::Io(error)),
        };

        file.write_all(&contents)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(profile)
    }

    pub fn load(&self) -> Result<Profile, ProfileError> {
        let file = File::open(&self.path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ProfileError::NotFound
            } else {
                ProfileError::Io(error)
            }
        })?;
        Ok(serde_json::from_reader(file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn creates_and_loads_a_profile_with_only_an_id() {
        let temp = tempdir().unwrap();
        let store = ProfileStore::new(temp.path());

        let created = store.create().unwrap();
        let loaded = store.load().unwrap();
        let json: Value =
            serde_json::from_slice(&fs::read(temp.path().join("profile.json")).unwrap()).unwrap();

        assert_eq!(created, loaded);
        assert_eq!(json.as_object().unwrap().len(), 1);
        assert_eq!(json["id"].as_str().unwrap(), created.id.to_string());
        assert!(created.id.get_version_num() == 7);
    }

    #[test]
    fn refuses_to_create_a_second_profile_in_the_same_workspace() {
        let temp = tempdir().unwrap();
        let store = ProfileStore::new(temp.path());

        store.create().unwrap();
        assert!(matches!(store.create(), Err(ProfileError::AlreadyExists)));
    }

    #[test]
    fn reports_a_missing_profile() {
        let temp = tempdir().unwrap();
        let store = ProfileStore::new(temp.path());

        assert!(matches!(store.load(), Err(ProfileError::NotFound)));
    }
}
