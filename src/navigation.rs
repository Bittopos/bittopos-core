use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use crate::Workspace;

/// Ubicación actual de navegación. La ausencia de una instancia representa la raíz.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_instance: Option<Uuid>,
}

#[derive(Debug)]
pub enum NavigationError {
    Io(io::Error),
    TomlParse(toml::de::Error),
    TomlSerialize(toml::ser::Error),
}

impl std::fmt::Display for NavigationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "error de E/S de navegación: {error}"),
            Self::TomlParse(error) => write!(f, "estado de navegación TOML inválido: {error}"),
            Self::TomlSerialize(error) => write!(f, "no se pudo serializar la navegación: {error}"),
        }
    }
}

impl std::error::Error for NavigationError {}

impl From<io::Error> for NavigationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Persistencia local del contexto de navegación.
pub struct NavigationStore {
    path: PathBuf,
}

impl NavigationStore {
    /// Guarda el estado en `navigation.toml` dentro del directorio de workspaces.
    pub fn new(workspaces_root: impl AsRef<Path>) -> Self {
        Self {
            path: workspaces_root.as_ref().join("navigation.toml"),
        }
    }

    /// Un archivo ausente representa la raíz. Un archivo malformado produce error.
    pub fn load(&self) -> Result<NavigationState, NavigationError> {
        match fs::read_to_string(&self.path) {
            Ok(content) => toml::from_str(&content).map_err(NavigationError::TomlParse),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(NavigationState::default()),
            Err(error) => Err(NavigationError::Io(error)),
        }
    }

    /// Recupera el estado y vuelve a la raíz si su destino ya no existe.
    /// Devuelve true cuando tuvo que corregir y persistir un destino inválido.
    pub fn load_validated(
        &self,
        workspace: &Workspace,
    ) -> Result<(NavigationState, bool), NavigationError> {
        let mut state = self.load()?;
        if state
            .current_instance
            .is_some_and(|id| workspace.find_instance(id).is_none())
        {
            state.current_instance = None;
            self.save(&state)?;
            return Ok((state, true));
        }
        Ok((state, false))
    }

    /// Persiste el estado de navegación.
    pub fn save(&self, state: &NavigationState) -> Result<(), NavigationError> {
        let content = toml::to_string_pretty(state).map_err(NavigationError::TomlSerialize)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, Instance, Workspace};

    #[test]
    fn persists_and_restores_current_instance() {
        let dir = tempfile::tempdir().unwrap();
        let store = NavigationStore::new(dir.path());
        let id = Uuid::now_v7();
        let workspace = Workspace {
            content: Content {
                instances: vec![Instance {
                    id,
                    source: crate::Source { id: Uuid::now_v7() },
                    semantic: None,
                    content: None,
                    masks: Vec::new(),
                }],
            },
        };

        store
            .save(&NavigationState {
                current_instance: Some(id),
            })
            .unwrap();
        let (loaded, corrected) = store.load_validated(&workspace).unwrap();
        assert_eq!(loaded.current_instance, Some(id));
        assert!(!corrected);
    }

    #[test]
    fn invalid_destination_is_reset_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = NavigationStore::new(dir.path());
        store
            .save(&NavigationState {
                current_instance: Some(Uuid::now_v7()),
            })
            .unwrap();

        let (loaded, corrected) = store.load_validated(&Workspace::new()).unwrap();
        assert_eq!(loaded.current_instance, None);
        assert!(corrected);
        assert_eq!(store.load().unwrap().current_instance, None);
    }

    #[test]
    fn malformed_toml_is_reported_without_overwriting_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = NavigationStore::new(dir.path());
        fs::write(dir.path().join("navigation.toml"), "current_instance = [").unwrap();

        assert!(store.load().is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("navigation.toml")).unwrap(),
            "current_instance = ["
        );
    }
}
