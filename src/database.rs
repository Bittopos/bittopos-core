//! SQLite persistence for the server and optional local use.
//!
//! TOML/JSON files remain valid interchange and user-managed formats. This module
//! stores queryable metadata and relational projections without replacing them.
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::ptr;

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open(filename: *const c_char, db: *mut *mut Sqlite3) -> c_int;
    fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    fn sqlite3_exec(
        db: *mut Sqlite3,
        sql: *const c_char,
        callback: Option<
            unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int,
        >,
        arg: *mut c_void,
        errmsg: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_free(ptr: *mut c_void);
}

const SQLITE_OK: c_int = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseError {
    Sql(String),
    Json(String),
    Invalid(String),
    NotFound(String),
    RevisionConflict { current: i64 },
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(message) => write!(f, "error de SQLite: {message}"),
            Self::Json(message) => write!(f, "JSON inválido: {message}"),
            Self::Invalid(message) => write!(f, "datos inválidos: {message}"),
            Self::NotFound(message) => write!(f, "no encontrado: {message}"),
            Self::RevisionConflict { current } => {
                write!(f, "la estructura cambió; revisión actual: {current}")
            }
        }
    }
}

impl std::error::Error for DatabaseError {}

/// SQLite connection. Keep one instance per server worker or protect it with a mutex.
pub struct Database {
    raw: *mut Sqlite3,
}

impl Drop for Database {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                sqlite3_close(self.raw);
            }
        }
    }
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        if let Some(parent) = path
            .as_ref()
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| DatabaseError::Sql(error.to_string()))?;
        }
        let path = CString::new(path.as_ref().to_string_lossy().as_bytes())
            .map_err(|_| DatabaseError::Invalid("la ruta contiene un byte NUL".into()))?;
        let mut raw = ptr::null_mut();
        let result = unsafe { sqlite3_open(path.as_ptr(), &mut raw) };
        if result != SQLITE_OK {
            let message = if raw.is_null() {
                format!("sqlite3_open devolvió {result}")
            } else {
                unsafe { CStr::from_ptr(sqlite3_errmsg(raw)) }
                    .to_string_lossy()
                    .into_owned()
            };
            if !raw.is_null() {
                unsafe { sqlite3_close(raw) };
            }
            return Err(DatabaseError::Sql(message));
        }
        let mut database = Self { raw };
        database.exec("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        database.exec(
            "CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                document_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                document_json TEXT NOT NULL,
                workspace_json TEXT NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS project_resources (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                document_json TEXT NOT NULL,
                source_toml TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY(project_id, id)
            );
            CREATE TABLE IF NOT EXISTS project_instances (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                id TEXT NOT NULL,
                parent_id TEXT,
                source_id TEXT NOT NULL,
                semantic_json TEXT,
                masks_json TEXT NOT NULL DEFAULT '[]',
                instance_json TEXT NOT NULL,
                PRIMARY KEY(project_id, id),
                FOREIGN KEY(project_id, parent_id)
                    REFERENCES project_instances(project_id, id)
                    ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
            );",
        )?;
        Ok(database)
    }

    pub fn create_profile(&mut self, profile: &serde_json::Value) -> Result<(), DatabaseError> {
        let id = required_text(profile, "id", "perfil")?;
        self.exec(&format!(
            "INSERT INTO profiles(id, document_json) VALUES ({}, {});",
            sql_text(id)?,
            sql_text(&json_text(profile)?)?
        ))
    }

    pub fn list_profiles(&mut self) -> Result<Vec<serde_json::Value>, DatabaseError> {
        self.query_json("SELECT document_json FROM profiles ORDER BY created_at, id")
    }

    pub fn create_project(&mut self, project: &serde_json::Value) -> Result<(), DatabaseError> {
        let id = required_text(project, "id", "proyecto")?;
        let profile_id = required_text(project, "profile_id", "proyecto")?;
        let name = required_text(project, "name", "proyecto")?.trim();
        if name.is_empty() {
            return Err(DatabaseError::Invalid(
                "el nombre del proyecto está vacío".into(),
            ));
        }
        let description = project
            .get("description")
            .and_then(serde_json::Value::as_str);
        self.exec(&format!(
            "INSERT INTO projects(id, profile_id, name, description, document_json, workspace_json)
             VALUES ({}, {}, {}, {}, {}, '{{\"content\":{{\"instances\":[]}}}}');",
            sql_text(id)?,
            sql_text(profile_id)?,
            sql_text(name)?,
            sql_optional_text(description)?,
            sql_text(&json_text(project)?)?
        ))
    }

    pub fn list_projects(
        &mut self,
        profile_id: &str,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        self.query_json(&format!(
            "SELECT document_json FROM projects WHERE profile_id={} ORDER BY name, id",
            sql_text(profile_id)?
        ))
    }

    pub fn save_resource(
        &mut self,
        project_id: &str,
        resource: &serde_json::Value,
        source_toml: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let id = required_text(resource, "id", "recurso")?;
        let info = resource
            .get("info")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| DatabaseError::Invalid("el recurso debe contener info".into()))?;
        let title = info
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .ok_or_else(|| DatabaseError::Invalid("el recurso debe contener info.title".into()))?;
        let description = info.get("description").and_then(serde_json::Value::as_str);
        let tags = info
            .get("tags")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        if !tags.is_array() || tags.as_array().unwrap().iter().any(|tag| !tag.is_string()) {
            return Err(DatabaseError::Invalid(
                "info.tags debe ser una lista de textos".into(),
            ));
        }
        self.exec(&format!(
            "INSERT INTO project_resources(project_id, id, title, description, tags_json, document_json, source_toml)
             VALUES ({}, {}, {}, {}, {}, {}, {})
             ON CONFLICT(project_id, id) DO UPDATE SET
               title=excluded.title, description=excluded.description, tags_json=excluded.tags_json,
               document_json=excluded.document_json, source_toml=excluded.source_toml,
               updated_at=CURRENT_TIMESTAMP;",
            sql_text(project_id)?,
            sql_text(id)?,
            sql_text(title.trim())?,
            sql_optional_text(description)?,
            sql_text(&json_text(&tags)?)?,
            sql_text(&json_text(resource)?)?,
            sql_optional_text(source_toml)?
        ))
    }

    pub fn list_resources(
        &mut self,
        project_id: &str,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        self.query_json(&format!(
            "SELECT document_json FROM project_resources WHERE project_id={} ORDER BY created_at, id",
            sql_text(project_id)?
        ))
    }

    /// Returns the exact TOML source retained on import, if this resource came from TOML.
    pub fn load_resource_toml(
        &mut self,
        project_id: &str,
        resource_id: &str,
    ) -> Result<Option<String>, DatabaseError> {
        let rows = self.query(&format!(
            "SELECT source_toml FROM project_resources WHERE project_id={} AND id={}",
            sql_text(project_id)?,
            sql_text(resource_id)?
        ))?;
        let row = rows
            .first()
            .ok_or_else(|| DatabaseError::NotFound(format!("recurso {resource_id}")))?;
        Ok(row.first().and_then(Clone::clone))
    }

    pub fn load_workspace(
        &mut self,
        project_id: &str,
    ) -> Result<(serde_json::Value, i64), DatabaseError> {
        let rows = self.query(&format!(
            "SELECT workspace_json, revision FROM projects WHERE id={}",
            sql_text(project_id)?
        ))?;
        let row = rows
            .first()
            .ok_or_else(|| DatabaseError::NotFound(format!("proyecto {project_id}")))?;
        let workspace = serde_json::from_str(row[0].as_deref().unwrap_or("{}"))
            .map_err(|error| DatabaseError::Json(error.to_string()))?;
        let revision = row[1]
            .as_deref()
            .unwrap_or("0")
            .parse::<i64>()
            .map_err(|error| DatabaseError::Sql(error.to_string()))?;
        Ok((workspace, revision))
    }

    pub fn save_workspace(
        &mut self,
        project_id: &str,
        workspace: &serde_json::Value,
        expected_revision: i64,
    ) -> Result<i64, DatabaseError> {
        validate_workspace(workspace)?;
        self.exec("BEGIN IMMEDIATE;")?;
        let result = (|| {
            let rows = self.query(&format!(
                "SELECT revision FROM projects WHERE id={}",
                sql_text(project_id)?
            ))?;
            let row = rows
                .first()
                .ok_or_else(|| DatabaseError::NotFound(format!("proyecto {project_id}")))?;
            let current = row[0]
                .as_deref()
                .unwrap_or("0")
                .parse::<i64>()
                .map_err(|error| DatabaseError::Sql(error.to_string()))?;
            if current != expected_revision {
                return Err(DatabaseError::RevisionConflict { current });
            }
            self.exec(&format!(
                "DELETE FROM project_instances WHERE project_id={};",
                sql_text(project_id)?
            ))?;
            let mut rows = Vec::new();
            flatten_instances(
                workspace["content"]["instances"].as_array().unwrap(),
                None,
                &mut rows,
            )?;
            for row in rows {
                self.exec(&format!(
                    "INSERT INTO project_instances(project_id, id, parent_id, source_id, semantic_json, masks_json, instance_json)
                     VALUES ({}, {}, {}, {}, {}, {}, {});",
                    sql_text(project_id)?,
                    sql_text(&row.id)?,
                    sql_optional_text(row.parent_id.as_deref())?,
                    sql_text(&row.source_id)?,
                    sql_optional_text(row.semantic.as_deref())?,
                    sql_text(&row.masks)?,
                    sql_text(&row.document)?
                ))?;
            }
            let next = current + 1;
            self.exec(&format!(
                "UPDATE projects SET workspace_json={}, revision={}, updated_at=CURRENT_TIMESTAMP WHERE id={};",
                sql_text(&json_text(workspace)?)?, next, sql_text(project_id)?
            ))?;
            Ok(next)
        })();
        match result {
            Ok(revision) => {
                self.exec("COMMIT;")?;
                Ok(revision)
            }
            Err(error) => {
                let _ = self.exec("ROLLBACK;");
                Err(error)
            }
        }
    }

    fn exec(&mut self, sql: &str) -> Result<(), DatabaseError> {
        let sql = CString::new(sql)
            .map_err(|_| DatabaseError::Invalid("la consulta contiene un byte NUL".into()))?;
        let mut error = ptr::null_mut();
        let result =
            unsafe { sqlite3_exec(self.raw, sql.as_ptr(), None, ptr::null_mut(), &mut error) };
        if result == SQLITE_OK {
            return Ok(());
        }
        let message = if error.is_null() {
            unsafe { CStr::from_ptr(sqlite3_errmsg(self.raw)) }
                .to_string_lossy()
                .into_owned()
        } else {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe { sqlite3_free(error.cast()) };
            message
        };
        Err(DatabaseError::Sql(message))
    }

    fn query(&mut self, sql: &str) -> Result<Vec<Vec<Option<String>>>, DatabaseError> {
        let sql = CString::new(sql)
            .map_err(|_| DatabaseError::Invalid("la consulta contiene un byte NUL".into()))?;
        let mut rows = Vec::<Vec<Option<String>>>::new();
        let mut error = ptr::null_mut();
        let result = unsafe {
            sqlite3_exec(
                self.raw,
                sql.as_ptr(),
                Some(collect_row),
                (&mut rows as *mut Vec<Vec<Option<String>>>).cast(),
                &mut error,
            )
        };
        if result == SQLITE_OK {
            return Ok(rows);
        }
        let message = if error.is_null() {
            unsafe { CStr::from_ptr(sqlite3_errmsg(self.raw)) }
                .to_string_lossy()
                .into_owned()
        } else {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe { sqlite3_free(error.cast()) };
            message
        };
        Err(DatabaseError::Sql(message))
    }

    fn query_json(&mut self, sql: &str) -> Result<Vec<serde_json::Value>, DatabaseError> {
        self.query(sql)?
            .iter()
            .map(|row| {
                serde_json::from_str(row.first().and_then(Option::as_deref).unwrap_or("{}"))
                    .map_err(|error| DatabaseError::Json(error.to_string()))
            })
            .collect()
    }
}

unsafe extern "C" fn collect_row(
    context: *mut c_void,
    count: c_int,
    values: *mut *mut c_char,
    _columns: *mut *mut c_char,
) -> c_int {
    if context.is_null() {
        return 1;
    }
    let rows = &mut *(context as *mut Vec<Vec<Option<String>>>);
    let values = if count == 0 || values.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(values, count as usize)
    };
    let row = values
        .iter()
        .map(|value| {
            if value.is_null() {
                None
            } else {
                Some(CStr::from_ptr(*value).to_string_lossy().into_owned())
            }
        })
        .collect();
    rows.push(row);
    0
}

struct FlatInstance {
    id: String,
    parent_id: Option<String>,
    source_id: String,
    semantic: Option<String>,
    masks: String,
    document: String,
}

fn flatten_instances(
    instances: &[serde_json::Value],
    parent_id: Option<&str>,
    output: &mut Vec<FlatInstance>,
) -> Result<(), DatabaseError> {
    for instance in instances {
        let id = required_text(instance, "id", "instancia")?.to_owned();
        let source_id = instance
            .get("source")
            .and_then(|source| source.get("id"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                DatabaseError::Invalid(format!("la instancia {id} debe contener source.id"))
            })?
            .to_owned();
        let semantic = instance
            .get("semantic")
            .filter(|value| !value.is_null())
            .map(json_text)
            .transpose()?;
        let masks = instance
            .get("masks")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let children = match instance.get("content") {
            None | Some(serde_json::Value::Null) => &[][..],
            Some(content) => content
                .get("instances")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .ok_or_else(|| {
                    DatabaseError::Invalid(format!("content.instances inválido en {id}"))
                })?,
        };
        output.push(FlatInstance {
            id: id.clone(),
            parent_id: parent_id.map(str::to_owned),
            source_id,
            semantic,
            masks: json_text(&masks)?,
            document: json_text(instance)?,
        });
        flatten_instances(children, Some(&id), output)?;
    }
    Ok(())
}

fn validate_workspace(workspace: &serde_json::Value) -> Result<(), DatabaseError> {
    let instances = workspace
        .get("content")
        .and_then(|content| content.get("instances"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            DatabaseError::Invalid("la estructura debe contener content.instances".into())
        })?;
    let mut rows = Vec::new();
    flatten_instances(instances, None, &mut rows)?;
    let mut ids = HashSet::new();
    for row in rows {
        if !ids.insert(row.id.clone()) {
            return Err(DatabaseError::Invalid(format!(
                "ID de instancia duplicado: {}",
                row.id
            )));
        }
    }
    Ok(())
}

fn required_text<'a>(
    value: &'a serde_json::Value,
    key: &str,
    entity: &str,
) -> Result<&'a str, DatabaseError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| DatabaseError::Invalid(format!("{entity} debe contener {key}")))
}

fn json_text(value: &serde_json::Value) -> Result<String, DatabaseError> {
    serde_json::to_string(value).map_err(|error| DatabaseError::Json(error.to_string()))
}

fn sql_text(value: &str) -> Result<String, DatabaseError> {
    if value.contains('\0') {
        return Err(DatabaseError::Invalid(
            "los textos no pueden contener NUL".into(),
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn sql_optional_text(value: Option<&str>) -> Result<String, DatabaseError> {
    match value {
        Some(value) => sql_text(value),
        None => Ok("NULL".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_profiles_projects_resources_and_original_toml() {
        let dir = tempdir().unwrap();
        let mut db = Database::open(dir.path().join("store.sqlite3")).unwrap();
        let profile = serde_json::json!({"id":"p1"});
        db.create_profile(&profile).unwrap();
        let project = serde_json::json!({"id":"j1","profile_id":"p1","name":"Study"});
        db.create_project(&project).unwrap();
        let resource = serde_json::json!({
            "id":"r1","info":{"title":"Definition","description":"Details","tags":["study"]},
            "custom":{"preserved":true}
        });
        let source = "id = \"r1\"\n[info]\ntitle = \"Definition\"\ndescription = \"Details\"\ntags = [\"study\"]\n";
        db.save_resource("j1", &resource, Some(source)).unwrap();
        assert_eq!(db.list_profiles().unwrap(), vec![profile]);
        assert_eq!(db.list_projects("p1").unwrap(), vec![project]);
        assert_eq!(db.list_resources("j1").unwrap(), vec![resource]);
        assert_eq!(
            db.load_resource_toml("j1", "r1").unwrap().as_deref(),
            Some(source)
        );
    }

    #[test]
    fn stores_instances_as_relational_rows_and_rejects_stale_revision() {
        let dir = tempdir().unwrap();
        let mut db = Database::open(dir.path().join("store.sqlite3")).unwrap();
        db.create_profile(&serde_json::json!({"id":"p1"})).unwrap();
        db.create_project(&serde_json::json!({"id":"j1","profile_id":"p1","name":"Study"}))
            .unwrap();
        let workspace = serde_json::json!({"content":{"instances":[
            {"id":"root","source":{"id":"r1"},"content":{"instances":[
                {"id":"child","source":{"id":"r2"},"masks":[{"segment":"info"}]}
            ]}}
        ]}});
        assert_eq!(db.save_workspace("j1", &workspace, 0).unwrap(), 1);
        assert!(matches!(
            db.save_workspace("j1", &workspace, 0),
            Err(DatabaseError::RevisionConflict { current: 1 })
        ));
        let rows = db.query("SELECT id, parent_id, source_id FROM project_instances WHERE project_id='j1' ORDER BY id").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].as_deref(), Some("child"));
        assert_eq!(rows[0][1].as_deref(), Some("root"));
    }
}
