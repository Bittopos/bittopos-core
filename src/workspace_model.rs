use crate::semantic_masks::{
    resolve, ComponentMerge, ElementAddress, InstanceSemantics, Mask, MaskError, MaskMode,
    ResourceDefinition,
};
use crate::Resource;
use std::collections::HashSet;
use uuid::Uuid;

/// Segmento físico que contiene instancias completas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    pub instances: Vec<Instance>,
}

/// Registro semántico de los IDs de los hijos directos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Semantic {
    pub allocation: Vec<Uuid>,
    /// Identidades de recursos de referencia asociados a esta instancia.
    pub references: Vec<Uuid>,
}

/// Referencia protegida al recurso del que procede una instancia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: Uuid,
}

/// Aparición concreta de un recurso dentro de un espacio de trabajo.
/// La identidad de instancia es independiente de la identidad de su recurso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub id: Uuid,
    pub source: Source,
    pub semantic: Option<Semantic>,
    pub content: Option<Content>,
    /// Máscaras semánticas locales persistidas junto a la instancia.
    pub masks: Vec<Mask>,
}

impl Instance {
    pub(crate) fn new(id: Uuid, source_id: Uuid) -> Self {
        Self {
            id,
            source: Source { id: source_id },
            semantic: None,
            content: None,
            masks: Vec::new(),
        }
    }

    pub fn children(&self) -> &[Instance] {
        self.content
            .as_ref()
            .map_or(&[], |content| content.instances.as_slice())
    }

    pub fn child_ids(&self) -> &[Uuid] {
        self.semantic
            .as_ref()
            .map_or(&[], |semantic| semantic.allocation.as_slice())
    }

    /// Vista semántica de las máscaras locales sin duplicar su estado persistido.
    pub fn mask_semantics(&self) -> InstanceSemantics {
        InstanceSemantics {
            id: self.id,
            source_id: self.source.id,
            masks: self.masks.clone(),
        }
    }

    /// Valida y aplica un estado semántico de máscaras a esta instancia.
    pub fn set_mask_semantics(&mut self, semantics: InstanceSemantics) -> Result<(), MaskError> {
        if semantics.id != self.id {
            return Err(MaskError::InstanceMismatch {
                expected: self.id,
                found: semantics.id,
            });
        }
        if semantics.source_id != self.source.id {
            return Err(MaskError::SourceMismatch {
                expected: self.source.id,
                found: semantics.source_id,
            });
        }
        semantics.validate()?;
        self.masks = semantics.masks;
        Ok(())
    }

    pub fn edit_local_concept(
        &mut self,
        resource: &Resource,
        segment: &str,
        concept: &str,
        value: impl Into<String>,
    ) -> Result<(), MaskError> {
        let source = Self::source_definition(resource)?;
        let mut semantics = self.mask_semantics();
        semantics.edit_local_concept(&source, segment, concept, value)?;
        self.set_mask_semantics(semantics)
    }

    pub fn edit_local_component(
        &mut self,
        resource: &Resource,
        segment: &str,
        component: &str,
        value: serde_json::Value,
    ) -> Result<(), MaskError> {
        let source = Self::source_definition(resource)?;
        let mut semantics = self.mask_semantics();
        semantics.edit_local_component(&source, segment, component, value)?;
        self.set_mask_semantics(semantics)
    }

    pub fn create_mask_snapshot(
        &mut self,
        resource: &Resource,
        segment: &str,
        scope: &[ElementAddress],
    ) -> Result<(), MaskError> {
        let source = Self::source_definition(resource)?;
        let mut semantics = self.mask_semantics();
        semantics.create_snapshot(&source, segment, scope)?;
        self.set_mask_semantics(semantics)
    }

    pub fn remove_semantic_mask(&mut self, segment: &str, mode: MaskMode) -> Result<(), MaskError> {
        let mut semantics = self.mask_semantics();
        semantics.remove_mask(segment, mode)?;
        self.set_mask_semantics(semantics)
    }

    pub fn resolved_semantics(
        &self,
        resource: &Resource,
        merge: &impl ComponentMerge,
    ) -> Result<ResourceDefinition, MaskError> {
        let source = Self::source_definition(resource)?;
        resolve(&source, &self.mask_semantics(), merge)
    }

    /// Construye la definición semántica almacenada en el recurso.
    pub fn source_definition(resource: &Resource) -> Result<ResourceDefinition, MaskError> {
        let definition = resource.semantic_definition();
        definition.validate()?;
        Ok(definition)
    }

    fn normalize_empty_children(&mut self) {
        if self
            .content
            .as_ref()
            .is_some_and(|content| content.instances.is_empty())
        {
            self.content = None;
        }
        if self.semantic.as_ref().is_some_and(|semantic| {
            semantic.allocation.is_empty() && semantic.references.is_empty()
        }) {
            self.semantic = None;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspace {
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceModelError {
    DuplicateInstanceId(Uuid),
    InvalidContainment(String),
    InstanceNotFound(Uuid),
    InstanceHasChildren(Uuid),
    ParentNotFound(Uuid),
    InvalidMask(String),
}

impl std::fmt::Display for WorkspaceModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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

impl std::error::Error for WorkspaceModelError {}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn instances(&self) -> &[Instance] {
        &self.content.instances
    }

    pub fn find_instance(&self, id: Uuid) -> Option<&Instance> {
        find_in(&self.content.instances, id)
    }

    pub fn find_instance_mut(&mut self, id: Uuid) -> Option<&mut Instance> {
        find_mut_in(&mut self.content.instances, id)
    }

    pub fn parent_of(&self, id: Uuid) -> Option<Uuid> {
        parent_in(&self.content.instances, id, None)
    }

    pub fn all_instances(&self) -> Vec<&Instance> {
        let mut output = Vec::new();
        collect_instances(&self.content.instances, &mut output);
        output
    }

    pub(crate) fn insert_root(&mut self, instance: Instance) -> Result<(), WorkspaceModelError> {
        if self.find_instance(instance.id).is_some() {
            return Err(WorkspaceModelError::DuplicateInstanceId(instance.id));
        }
        self.content.instances.push(instance);
        Ok(())
    }

    pub(crate) fn insert_child(
        &mut self,
        parent_id: Uuid,
        instance: Instance,
    ) -> Result<(), WorkspaceModelError> {
        if self.find_instance(instance.id).is_some() {
            return Err(WorkspaceModelError::DuplicateInstanceId(instance.id));
        }
        let parent = find_mut_in(&mut self.content.instances, parent_id)
            .ok_or(WorkspaceModelError::ParentNotFound(parent_id))?;
        let child_id = instance.id;
        parent
            .content
            .get_or_insert_with(Content::default)
            .instances
            .push(instance);
        parent
            .semantic
            .get_or_insert_with(Semantic::default)
            .allocation
            .push(child_id);
        Ok(())
    }

    /// Remueve una instancia hoja a cualquier profundidad. Nunca elimina descendientes implícitamente.
    pub(crate) fn remove_leaf(&mut self, id: Uuid) -> Result<Instance, WorkspaceModelError> {
        let parent_id = self.parent_of(id);
        if let Some(parent_id) = parent_id {
            let parent = find_mut_in(&mut self.content.instances, parent_id)
                .ok_or(WorkspaceModelError::ParentNotFound(parent_id))?;
            let content = parent
                .content
                .as_mut()
                .ok_or(WorkspaceModelError::InstanceNotFound(id))?;
            let index = content
                .instances
                .iter()
                .position(|child| child.id == id)
                .ok_or(WorkspaceModelError::InstanceNotFound(id))?;
            if !content.instances[index].children().is_empty() {
                return Err(WorkspaceModelError::InstanceHasChildren(id));
            }
            let removed = content.instances.remove(index);
            if let Some(semantic) = parent.semantic.as_mut() {
                semantic.allocation.retain(|child_id| *child_id != id);
            }
            parent.normalize_empty_children();
            Ok(removed)
        } else {
            let index = self
                .content
                .instances
                .iter()
                .position(|instance| instance.id == id)
                .ok_or(WorkspaceModelError::InstanceNotFound(id))?;
            if !self.content.instances[index].children().is_empty() {
                return Err(WorkspaceModelError::InstanceHasChildren(id));
            }
            Ok(self.content.instances.remove(index))
        }
    }

    pub fn validate(&self) -> Result<(), WorkspaceModelError> {
        let mut seen = HashSet::new();
        validate_instances(&self.content.instances, &mut seen)
    }
}

fn find_in(instances: &[Instance], id: Uuid) -> Option<&Instance> {
    for instance in instances {
        if instance.id == id {
            return Some(instance);
        }
        if let Some(found) = find_in(instance.children(), id) {
            return Some(found);
        }
    }
    None
}

fn find_mut_in(instances: &mut [Instance], id: Uuid) -> Option<&mut Instance> {
    for instance in instances {
        if instance.id == id {
            return Some(instance);
        }
        if let Some(content) = instance.content.as_mut() {
            if let Some(found) = find_mut_in(content.instances.as_mut_slice(), id) {
                return Some(found);
            }
        }
    }
    None
}

fn parent_in(instances: &[Instance], id: Uuid, parent: Option<Uuid>) -> Option<Uuid> {
    for instance in instances {
        if instance.id == id {
            return parent;
        }
        if let Some(found) = parent_in(instance.children(), id, Some(instance.id)) {
            return Some(found);
        }
    }
    None
}

fn collect_instances<'a>(instances: &'a [Instance], output: &mut Vec<&'a Instance>) {
    for instance in instances {
        output.push(instance);
        collect_instances(instance.children(), output);
    }
}

fn validate_instances(
    instances: &[Instance],
    seen: &mut HashSet<Uuid>,
) -> Result<(), WorkspaceModelError> {
    for instance in instances {
        instance
            .mask_semantics()
            .validate()
            .map_err(|error| WorkspaceModelError::InvalidMask(error.to_string()))?;
        if !seen.insert(instance.id) {
            return Err(WorkspaceModelError::DuplicateInstanceId(instance.id));
        }
        let children = instance.children();
        let allocation = instance.child_ids();
        if children.is_empty() {
            if !allocation.is_empty() {
                return Err(WorkspaceModelError::InvalidContainment(format!(
                    "la instancia {} registra hijos semánticos pero no contiene hijos físicos",
                    instance.id
                )));
            }
            continue;
        }
        if instance.semantic.is_none() {
            return Err(WorkspaceModelError::InvalidContainment(format!(
                "la instancia {} contiene hijos físicos, pero falta semantic.allocation",
                instance.id
            )));
        }
        let child_ids: Vec<_> = children.iter().map(|child| child.id).collect();
        if allocation != child_ids.as_slice() {
            return Err(WorkspaceModelError::InvalidContainment(format!(
                "semantic.allocation no coincide en contenido y orden con content.instances para {}", instance.id
            )));
        }
        validate_instances(children, seen)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_model_rejects_duplicate_instance_ids() {
        let id = Uuid::now_v7();
        let workspace = Workspace {
            content: Content {
                instances: vec![
                    Instance::new(id, Uuid::now_v7()),
                    Instance::new(id, Uuid::now_v7()),
                ],
            },
        };

        assert_eq!(
            workspace.validate(),
            Err(WorkspaceModelError::DuplicateInstanceId(id))
        );
    }

    #[test]
    fn workspace_model_rejects_semantic_physical_mismatch() {
        let mut root = Instance::new(Uuid::now_v7(), Uuid::now_v7());
        root.content = Some(Content {
            instances: vec![Instance::new(Uuid::now_v7(), Uuid::now_v7())],
        });
        root.semantic = Some(Semantic {
            allocation: vec![Uuid::now_v7()],
            references: Vec::new(),
        });

        let workspace = Workspace {
            content: Content {
                instances: vec![root],
            },
        };

        assert!(matches!(
            workspace.validate(),
            Err(WorkspaceModelError::InvalidContainment(_))
        ));
    }

    #[test]
    fn removing_leaf_updates_physical_and_semantic_containment() {
        let resource_id = Uuid::now_v7();
        let child_id = Uuid::now_v7();
        let leaf_id = Uuid::now_v7();

        let mut child = Instance::new(child_id, resource_id);
        child.content = Some(Content {
            instances: vec![Instance::new(leaf_id, resource_id)],
        });
        child.semantic = Some(Semantic {
            allocation: vec![leaf_id],
            references: Vec::new(),
        });

        let mut root = Instance::new(Uuid::now_v7(), resource_id);
        root.content = Some(Content {
            instances: vec![child],
        });
        root.semantic = Some(Semantic {
            allocation: vec![child_id],
            references: Vec::new(),
        });

        let mut workspace = Workspace {
            content: Content {
                instances: vec![root],
            },
        };

        workspace.remove_leaf(leaf_id).unwrap();
        workspace.validate().unwrap();

        assert!(workspace.find_instance(leaf_id).is_none());
        assert!(workspace
            .find_instance(child_id)
            .unwrap()
            .children()
            .is_empty());
        assert!(workspace
            .find_instance(child_id)
            .unwrap()
            .child_ids()
            .is_empty());
    }

    #[test]
    fn cannot_remove_instance_that_has_children() {
        let resource_id = Uuid::now_v7();
        let child_id = Uuid::now_v7();
        let mut root = Instance::new(Uuid::now_v7(), resource_id);
        root.content = Some(Content {
            instances: vec![Instance::new(child_id, resource_id)],
        });
        root.semantic = Some(Semantic {
            allocation: vec![child_id],
            references: Vec::new(),
        });

        let mut workspace = Workspace {
            content: Content {
                instances: vec![root],
            },
        };
        let root_id = workspace.content.instances[0].id;

        assert!(matches!(
            workspace.remove_leaf(root_id),
            Err(WorkspaceModelError::InstanceHasChildren(id)) if id == root_id
        ));
        assert!(workspace.find_instance(child_id).is_some());
    }
}
