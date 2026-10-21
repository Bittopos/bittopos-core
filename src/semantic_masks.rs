//! Modelo de dominio puro para máscaras semánticas.
//!
//! La resolución parte de la definición vigente y aplica solo las diferencias
//! locales o los valores congelados declarados por la instancia.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDefinition {
    pub id: Uuid,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSemantics {
    pub id: Uuid,
    pub source_id: Uuid,
    pub masks: Vec<Mask>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentRole {
    #[default]
    Semantic,
    Control,
    Permissions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub key: String,
    #[serde(default)]
    pub role: SegmentRole,
    pub concepts: Vec<Concept>,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concept {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub key: String,
    pub kind: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementAddress {
    Concept(String),
    Component(String),
}

impl ElementAddress {
    fn key(&self) -> &str {
        match self {
            Self::Concept(key) | Self::Component(key) => key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskMode {
    Point,
    Snapshot,
}

/// Una máscara es un segmento local de la instancia con al menos un elemento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mask {
    pub segment: Segment,
    pub target_segment: String,
    pub mode: MaskMode,
    /// En Snapshot enumera exactamente los elementos congelados.
    pub scope: Vec<ElementAddress>,
}

/// La combinación de componentes depende del tipo; la regla se inyecta al resolver.
pub trait ComponentMerge {
    fn merge(&self, inherited: &Component, local: &Component) -> Result<Component, String>;
}

/// Estrategia explícita de reemplazo completo para tipos que admiten ese comportamiento.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReplaceComponent;

impl ComponentMerge for ReplaceComponent {
    fn merge(&self, inherited: &Component, local: &Component) -> Result<Component, String> {
        if inherited.kind != local.kind {
            return Err(format!(
                "el componente '{}' cambia de tipo '{}' a '{}'",
                inherited.key, inherited.kind, local.kind
            ));
        }
        Ok(local.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskError {
    SourceMismatch {
        expected: Uuid,
        found: Uuid,
    },
    InstanceMismatch {
        expected: Uuid,
        found: Uuid,
    },
    EmptyKey,
    DuplicateSegment(String),
    DuplicateConcept {
        segment: String,
        key: String,
    },
    DuplicateComponent {
        segment: String,
        key: String,
    },
    InvalidComponentValue {
        segment: String,
        key: String,
        kind: String,
    },
    EmptyMask,
    DuplicateMaskTarget(String),
    MaskNotFound {
        target_segment: String,
        mode: MaskMode,
    },
    SegmentNotFound(String),
    ConceptNotFound {
        segment: String,
        key: String,
    },
    ComponentNotFound {
        segment: String,
        key: String,
    },
    InvalidSnapshotScope(String),
    SnapshotSourceChanged {
        segment: String,
        key: String,
    },
    SourceElementMissing {
        segment: String,
        key: String,
    },
    OverlappingMasks {
        segment: String,
        key: String,
    },
    ComponentMerge(String),
}

impl std::fmt::Display for MaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceMismatch { expected, found } => write!(f, "la instancia apunta al recurso {expected}, pero se recibió {found}"),
            Self::InstanceMismatch { expected, found } => write!(f, "la máscara pertenece a la instancia {found}, no a {expected}"),
            Self::EmptyKey => write!(f, "las claves semánticas no pueden estar vacías"),
            Self::DuplicateSegment(key) => write!(f, "segmento duplicado: {key}"),
            Self::DuplicateConcept { segment, key } => write!(f, "concepto duplicado '{key}' en el segmento '{segment}'"),
            Self::DuplicateComponent { segment, key } => write!(f, "componente duplicado '{key}' en el segmento '{segment}'"),
            Self::InvalidComponentValue { segment, key, kind } => write!(f, "el valor del componente '{key}' en '{segment}' no cumple el tipo '{kind}'"),
            Self::EmptyMask => write!(f, "una máscara debe contener al menos un concepto o componente"),
            Self::DuplicateMaskTarget(target) => write!(f, "hay más de una máscara del mismo modo para el segmento '{target}'"),
            Self::MaskNotFound { target_segment, mode } => write!(f, "no existe máscara {mode:?} para el segmento '{target_segment}'"),
            Self::SegmentNotFound(key) => write!(f, "no existe el segmento semántico '{key}'"),
            Self::ConceptNotFound { segment, key } => write!(f, "no existe el concepto '{key}' en el segmento '{segment}'"),
            Self::ComponentNotFound { segment, key } => write!(f, "no existe el componente '{key}' en el segmento '{segment}'"),
            Self::InvalidSnapshotScope(key) => write!(f, "el alcance Snapshot es inválido o está duplicado: {key}"),
            Self::SnapshotSourceChanged { segment, key } => write!(f, "el elemento congelado '{key}' ya no está en el segmento de origen '{segment}'; el caso requiere una decisión explícita"),
            Self::SourceElementMissing { segment, key } => write!(f, "el elemento enmascarado '{key}' ya no está en el segmento de origen '{segment}'; no se infiere una regla de eliminación"),
            Self::OverlappingMasks { segment, key } => write!(f, "máscaras superpuestas para '{key}' en el segmento '{segment}'"),
            Self::ComponentMerge(message) => write!(f, "no se pudo combinar el componente: {message}"),
        }
    }
}
impl std::error::Error for MaskError {}

impl ResourceDefinition {
    pub fn validate(&self) -> Result<(), MaskError> {
        let mut seen = HashSet::new();
        for segment in &self.segments {
            validate_segment(segment)?;
            if !seen.insert(segment.key.as_str()) {
                return Err(MaskError::DuplicateSegment(segment.key.clone()));
            }
        }
        Ok(())
    }

    pub fn segment(&self, key: &str) -> Option<&Segment> {
        self.segments.iter().find(|segment| segment.key == key)
    }

    pub fn segment_mut(&mut self, key: &str) -> Option<&mut Segment> {
        self.segments.iter_mut().find(|segment| segment.key == key)
    }

    /// Itera solo los segmentos semánticos presentables como contenido ordinario.
    pub fn visible_segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments
            .iter()
            .filter(|segment| segment.role == SegmentRole::Semantic)
    }

    /// Editar la definición sin máscara cambia el recurso compartido.
    pub fn edit_shared_concept(
        &mut self,
        segment_key: &str,
        concept_key: &str,
        value: impl Into<String>,
    ) -> Result<(), MaskError> {
        let segment = self
            .segment_mut(segment_key)
            .ok_or_else(|| MaskError::SegmentNotFound(segment_key.to_owned()))?;
        let concept = segment
            .concepts
            .iter_mut()
            .find(|item| item.key == concept_key)
            .ok_or_else(|| MaskError::ConceptNotFound {
                segment: segment_key.to_owned(),
                key: concept_key.to_owned(),
            })?;
        concept.value = value.into();
        Ok(())
    }

    pub fn edit_shared_component(
        &mut self,
        segment_key: &str,
        component_key: &str,
        value: serde_json::Value,
    ) -> Result<(), MaskError> {
        let segment = self
            .segment_mut(segment_key)
            .ok_or_else(|| MaskError::SegmentNotFound(segment_key.to_owned()))?;
        let component = segment
            .components
            .iter_mut()
            .find(|item| item.key == component_key)
            .ok_or_else(|| MaskError::ComponentNotFound {
                segment: segment_key.to_owned(),
                key: component_key.to_owned(),
            })?;
        component.value = value;
        Ok(())
    }
}

impl InstanceSemantics {
    pub fn new(id: Uuid, source_id: Uuid) -> Self {
        Self {
            id,
            source_id,
            masks: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), MaskError> {
        let mut targets = HashSet::new();
        for mask in &self.masks {
            validate_mask(mask)?;
            if !targets.insert((mask.target_segment.as_str(), mask.mode)) {
                return Err(MaskError::DuplicateMaskTarget(mask.target_segment.clone()));
            }
        }
        for point in self.masks.iter().filter(|m| m.mode == MaskMode::Point) {
            if let Some(snapshot) = self
                .masks
                .iter()
                .find(|m| m.target_segment == point.target_segment && m.mode == MaskMode::Snapshot)
            {
                for concept in &point.segment.concepts {
                    if snapshot
                        .scope
                        .contains(&ElementAddress::Concept(concept.key.clone()))
                    {
                        return Err(MaskError::OverlappingMasks {
                            segment: point.target_segment.clone(),
                            key: concept.key.clone(),
                        });
                    }
                }
                for component in &point.segment.components {
                    if snapshot
                        .scope
                        .contains(&ElementAddress::Component(component.key.clone()))
                    {
                        return Err(MaskError::OverlappingMasks {
                            segment: point.target_segment.clone(),
                            key: component.key.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Crea o actualiza una diferencia local; si coincide con el origen, la retira.
    pub fn edit_local_concept(
        &mut self,
        source: &ResourceDefinition,
        segment_key: &str,
        concept_key: &str,
        value: impl Into<String>,
    ) -> Result<(), MaskError> {
        ensure_source(self, source)?;
        let source_segment = source
            .segment(segment_key)
            .ok_or_else(|| MaskError::SegmentNotFound(segment_key.to_owned()))?;
        let inherited = source_segment
            .concepts
            .iter()
            .find(|item| item.key == concept_key)
            .ok_or_else(|| MaskError::ConceptNotFound {
                segment: segment_key.to_owned(),
                key: concept_key.to_owned(),
            })?;
        let value = value.into();

        if let Some(mask) = self
            .masks
            .iter()
            .find(|mask| mask.target_segment == segment_key && mask.mode == MaskMode::Snapshot)
        {
            if mask
                .scope
                .contains(&ElementAddress::Concept(concept_key.to_owned()))
            {
                return Err(MaskError::OverlappingMasks {
                    segment: segment_key.to_owned(),
                    key: concept_key.to_owned(),
                });
            }
        }
        if value == inherited.value {
            self.remove_point_concept(segment_key, concept_key);
            return Ok(());
        }

        let mask = self.point_mask_mut(segment_key);
        if let Some(item) = mask
            .segment
            .concepts
            .iter_mut()
            .find(|item| item.key == concept_key)
        {
            item.value = value;
        } else {
            mask.segment.concepts.push(Concept {
                key: concept_key.to_owned(),
                value,
            });
        }
        Ok(())
    }

    /// Crea o actualiza una diferencia local para un componente tipado.
    pub fn edit_local_component(
        &mut self,
        source: &ResourceDefinition,
        segment_key: &str,
        component_key: &str,
        value: serde_json::Value,
    ) -> Result<(), MaskError> {
        ensure_source(self, source)?;
        let source_segment = source
            .segment(segment_key)
            .ok_or_else(|| MaskError::SegmentNotFound(segment_key.to_owned()))?;
        let inherited = source_segment
            .components
            .iter()
            .find(|item| item.key == component_key)
            .ok_or_else(|| MaskError::ComponentNotFound {
                segment: segment_key.to_owned(),
                key: component_key.to_owned(),
            })?;
        if let Some(mask) = self
            .masks
            .iter()
            .find(|m| m.target_segment == segment_key && m.mode == MaskMode::Snapshot)
        {
            if mask
                .scope
                .contains(&ElementAddress::Component(component_key.to_owned()))
            {
                return Err(MaskError::OverlappingMasks {
                    segment: segment_key.to_owned(),
                    key: component_key.to_owned(),
                });
            }
        }
        if inherited.value == value {
            self.remove_point_component(segment_key, component_key);
            return Ok(());
        }
        let local = Component {
            key: component_key.to_owned(),
            kind: inherited.kind.clone(),
            value,
        };
        let mask = self.point_mask_mut(segment_key);
        if let Some(existing) = mask
            .segment
            .components
            .iter_mut()
            .find(|item| item.key == component_key)
        {
            *existing = local;
        } else {
            mask.segment.components.push(local);
        }
        Ok(())
    }

    /// Retira una diferencia puntual de componente.
    pub fn remove_point_component(&mut self, segment_key: &str, component_key: &str) {
        if let Some(mask) = self
            .masks
            .iter_mut()
            .find(|m| m.target_segment == segment_key && m.mode == MaskMode::Point)
        {
            mask.segment
                .components
                .retain(|item| item.key != component_key);
        }
        self.masks
            .retain(|m| !m.segment.concepts.is_empty() || !m.segment.components.is_empty());
    }

    /// Quita una diferencia puntual; si el segmento-máscara queda vacío, lo retira.
    pub fn remove_point_concept(&mut self, segment_key: &str, concept_key: &str) {
        if let Some(mask) = self
            .masks
            .iter_mut()
            .find(|mask| mask.target_segment == segment_key && mask.mode == MaskMode::Point)
        {
            mask.segment.concepts.retain(|item| item.key != concept_key);
        }
        self.masks.retain(|mask| {
            !mask.segment.concepts.is_empty() || !mask.segment.components.is_empty()
        });
    }

    /// Congela exclusivamente los elementos indicados en scope.
    pub fn create_snapshot(
        &mut self,
        source: &ResourceDefinition,
        segment_key: &str,
        scope: &[ElementAddress],
    ) -> Result<(), MaskError> {
        ensure_source(self, source)?;
        let source_segment = source
            .segment(segment_key)
            .ok_or_else(|| MaskError::SegmentNotFound(segment_key.to_owned()))?;
        if scope.is_empty() {
            return Err(MaskError::EmptyMask);
        }
        if let Some(point) = self
            .masks
            .iter()
            .find(|m| m.target_segment == segment_key && m.mode == MaskMode::Point)
        {
            for address in scope {
                let overlaps = match address {
                    ElementAddress::Concept(key) => {
                        point.segment.concepts.iter().any(|item| item.key == *key)
                    }
                    ElementAddress::Component(key) => {
                        point.segment.components.iter().any(|item| item.key == *key)
                    }
                };
                if overlaps {
                    return Err(MaskError::OverlappingMasks {
                        segment: segment_key.to_owned(),
                        key: address.key().to_owned(),
                    });
                }
            }
        }

        let mut seen = HashSet::new();
        let mut snapshot = Segment {
            role: SegmentRole::Semantic,
            key: format!("mask/{segment_key}/snapshot"),
            concepts: Vec::new(),
            components: Vec::new(),
        };
        for address in scope {
            let signature = match address {
                ElementAddress::Concept(key) => format!("concept:{key}"),
                ElementAddress::Component(key) => format!("component:{key}"),
            };
            if !seen.insert(signature.clone()) {
                return Err(MaskError::InvalidSnapshotScope(signature));
            }
            match address {
                ElementAddress::Concept(key) => {
                    let item = source_segment
                        .concepts
                        .iter()
                        .find(|item| item.key == *key)
                        .ok_or_else(|| MaskError::ConceptNotFound {
                            segment: segment_key.to_owned(),
                            key: key.clone(),
                        })?;
                    snapshot.concepts.push(item.clone());
                }
                ElementAddress::Component(key) => {
                    let item = source_segment
                        .components
                        .iter()
                        .find(|item| item.key == *key)
                        .ok_or_else(|| MaskError::ComponentNotFound {
                            segment: segment_key.to_owned(),
                            key: key.clone(),
                        })?;
                    snapshot.components.push(item.clone());
                }
            }
        }
        self.masks.retain(|mask| {
            !(mask.target_segment == segment_key && mask.mode == MaskMode::Snapshot)
        });
        self.masks.push(Mask {
            segment: snapshot,
            target_segment: segment_key.to_owned(),
            mode: MaskMode::Snapshot,
            scope: scope.to_vec(),
        });
        self.validate()
    }

    pub fn remove_mask(&mut self, segment_key: &str, mode: MaskMode) -> Result<(), MaskError> {
        let old_len = self.masks.len();
        self.masks
            .retain(|mask| !(mask.target_segment == segment_key && mask.mode == mode));
        if old_len == self.masks.len() {
            return Err(MaskError::MaskNotFound {
                target_segment: segment_key.to_owned(),
                mode,
            });
        }
        Ok(())
    }

    fn point_mask_mut(&mut self, segment_key: &str) -> &mut Mask {
        if let Some(index) = self
            .masks
            .iter()
            .position(|mask| mask.target_segment == segment_key && mask.mode == MaskMode::Point)
        {
            return &mut self.masks[index];
        }
        self.masks.push(Mask {
            segment: Segment {
                role: SegmentRole::Semantic,
                key: format!("mask/{segment_key}/point"),
                concepts: Vec::new(),
                components: Vec::new(),
            },
            target_segment: segment_key.to_owned(),
            mode: MaskMode::Point,
            scope: Vec::new(),
        });
        self.masks.last_mut().expect("mask was just inserted")
    }
}

/// Resuelve sobre la definición vigente. Los casos de eliminación o reubicación
/// del origen fallan explícitamente en lugar de inventar una política implícita.
pub fn resolve(
    source: &ResourceDefinition,
    instance: &InstanceSemantics,
    component_merge: &impl ComponentMerge,
) -> Result<ResourceDefinition, MaskError> {
    ensure_source(instance, source)?;
    source.validate()?;
    instance.validate()?;
    let mut resolved = source.clone();

    for mask in &instance.masks {
        let source_segment = source
            .segment(&mask.target_segment)
            .ok_or_else(|| MaskError::SegmentNotFound(mask.target_segment.clone()))?;
        match mask.mode {
            MaskMode::Point => {
                for local in &mask.segment.concepts {
                    source_segment
                        .concepts
                        .iter()
                        .find(|item| item.key == local.key)
                        .ok_or_else(|| MaskError::SourceElementMissing {
                            segment: mask.target_segment.clone(),
                            key: local.key.clone(),
                        })?;
                }
                for local in &mask.segment.components {
                    let inherited = source_segment
                        .components
                        .iter()
                        .find(|item| item.key == local.key)
                        .ok_or_else(|| MaskError::SourceElementMissing {
                            segment: mask.target_segment.clone(),
                            key: local.key.clone(),
                        })?;
                    if inherited.kind != local.kind {
                        return Err(MaskError::ComponentMerge(format!(
                            "el componente '{}' cambia de tipo '{}' a '{}'",
                            local.key, inherited.kind, local.kind
                        )));
                    }
                }
            }
            MaskMode::Snapshot => {
                for address in &mask.scope {
                    let exists = match address {
                        ElementAddress::Concept(key) => {
                            source_segment.concepts.iter().any(|item| item.key == *key)
                        }
                        ElementAddress::Component(key) => source_segment
                            .components
                            .iter()
                            .any(|item| item.key == *key),
                    };
                    if !exists {
                        return Err(MaskError::SnapshotSourceChanged {
                            segment: mask.target_segment.clone(),
                            key: address.key().to_owned(),
                        });
                    }
                }
            }
        }

        let target = resolved
            .segment_mut(&mask.target_segment)
            .ok_or_else(|| MaskError::SegmentNotFound(mask.target_segment.clone()))?;
        for local in &mask.segment.concepts {
            if let Some(inherited) = target
                .concepts
                .iter_mut()
                .find(|item| item.key == local.key)
            {
                inherited.value = local.value.clone();
            } else {
                return Err(MaskError::SourceElementMissing {
                    segment: mask.target_segment.clone(),
                    key: local.key.clone(),
                });
            }
        }
        for local in &mask.segment.components {
            let index = target
                .components
                .iter()
                .position(|item| item.key == local.key)
                .ok_or_else(|| MaskError::SourceElementMissing {
                    segment: mask.target_segment.clone(),
                    key: local.key.clone(),
                })?;
            let inherited = &target.components[index];
            let merged = match mask.mode {
                MaskMode::Point => component_merge
                    .merge(inherited, local)
                    .map_err(MaskError::ComponentMerge)?,
                MaskMode::Snapshot => local.clone(),
            };
            target.components[index] = merged;
        }
    }
    Ok(resolved)
}

fn ensure_source(
    instance: &InstanceSemantics,
    source: &ResourceDefinition,
) -> Result<(), MaskError> {
    if instance.source_id != source.id {
        return Err(MaskError::SourceMismatch {
            expected: instance.source_id,
            found: source.id,
        });
    }
    Ok(())
}

fn validate_segment(segment: &Segment) -> Result<(), MaskError> {
    if segment.key.trim().is_empty() {
        return Err(MaskError::EmptyKey);
    }
    let mut concepts = HashSet::new();
    for item in &segment.concepts {
        if item.key.trim().is_empty() {
            return Err(MaskError::EmptyKey);
        }
        if !concepts.insert(item.key.as_str()) {
            return Err(MaskError::DuplicateConcept {
                segment: segment.key.clone(),
                key: item.key.clone(),
            });
        }
    }
    let mut components = HashSet::new();
    for item in &segment.components {
        if item.key.trim().is_empty() || item.kind.trim().is_empty() {
            return Err(MaskError::EmptyKey);
        }
        if !components.insert(item.key.as_str()) {
            return Err(MaskError::DuplicateComponent {
                segment: segment.key.clone(),
                key: item.key.clone(),
            });
        }
        if item.kind == "string-list"
            && !matches!(&item.value, serde_json::Value::Array(values) if values.iter().all(serde_json::Value::is_string))
        {
            return Err(MaskError::InvalidComponentValue {
                segment: segment.key.clone(),
                key: item.key.clone(),
                kind: item.kind.clone(),
            });
        }
    }
    Ok(())
}

fn validate_mask(mask: &Mask) -> Result<(), MaskError> {
    validate_segment(&mask.segment)?;
    if mask.target_segment.trim().is_empty() {
        return Err(MaskError::EmptyKey);
    }
    if mask.segment.concepts.is_empty() && mask.segment.components.is_empty() {
        return Err(MaskError::EmptyMask);
    }
    if mask.mode == MaskMode::Point && !mask.scope.is_empty() {
        return Err(MaskError::InvalidSnapshotScope(
            "una máscara puntual no puede declarar alcance Snapshot".to_owned(),
        ));
    }
    if mask.mode == MaskMode::Snapshot {
        if mask.scope.is_empty() {
            return Err(MaskError::EmptyMask);
        }
        for address in &mask.scope {
            let present = match address {
                ElementAddress::Concept(key) => {
                    mask.segment.concepts.iter().any(|item| item.key == *key)
                }
                ElementAddress::Component(key) => {
                    mask.segment.components.iter().any(|item| item.key == *key)
                }
            };
            if !present {
                return Err(MaskError::InvalidSnapshotScope(address.key().to_owned()));
            }
        }
        for concept in &mask.segment.concepts {
            if !mask
                .scope
                .contains(&ElementAddress::Concept(concept.key.clone()))
            {
                return Err(MaskError::InvalidSnapshotScope(concept.key.clone()));
            }
        }
        for component in &mask.segment.components {
            if !mask
                .scope
                .contains(&ElementAddress::Component(component.key.clone()))
            {
                return Err(MaskError::InvalidSnapshotScope(component.key.clone()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source() -> ResourceDefinition {
        ResourceDefinition {
            id: Uuid::from_u128(1),
            segments: vec![Segment {
                role: SegmentRole::Semantic,
                key: "info".to_owned(),
                concepts: vec![
                    Concept {
                        key: "title".to_owned(),
                        value: "Original".to_owned(),
                    },
                    Concept {
                        key: "description".to_owned(),
                        value: "Description v1".to_owned(),
                    },
                ],
                components: vec![Component {
                    key: "link".to_owned(),
                    kind: "replaceable-link".to_owned(),
                    value: json!({"url": "https://example.test/v1"}),
                }],
            }],
        }
    }

    fn instance(source: &ResourceDefinition) -> InstanceSemantics {
        InstanceSemantics::new(Uuid::from_u128(2), source.id)
    }

    #[test]
    fn unmasked_values_follow_the_current_definition() {
        let mut source = source();
        let local = instance(&source);
        source
            .edit_shared_concept("info", "title", "Redefined")
            .unwrap();
        assert_eq!(
            resolve(&source, &local, &ReplaceComponent)
                .unwrap()
                .segment("info")
                .unwrap()
                .concepts[0]
                .value,
            "Redefined"
        );
    }

    #[test]
    fn edits_without_masks_change_the_shared_definition() {
        let mut source = source();
        let first = instance(&source);
        let second = instance(&source);
        source
            .edit_shared_concept("info", "title", "Shared edit")
            .unwrap();
        assert_eq!(
            resolve(&source, &first, &ReplaceComponent)
                .unwrap()
                .segment("info")
                .unwrap()
                .concepts[0]
                .value,
            "Shared edit"
        );
        assert_eq!(
            resolve(&source, &second, &ReplaceComponent)
                .unwrap()
                .segment("info")
                .unwrap()
                .concepts[0]
                .value,
            "Shared edit"
        );
    }

    #[test]
    fn point_mask_changes_only_the_local_instance() {
        let source = source();
        let mut local = instance(&source);
        local
            .edit_local_concept(&source, "info", "title", "Local title")
            .unwrap();
        let resolved = resolve(&source, &local, &ReplaceComponent).unwrap();
        assert_eq!(
            resolved.segment("info").unwrap().concepts[0].value,
            "Local title"
        );
        assert_eq!(
            source.segment("info").unwrap().concepts[0].value,
            "Original"
        );
        assert_eq!(local.masks[0].segment.concepts.len(), 1);
    }

    #[test]
    fn source_changes_flow_through_unmasked_values_only() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .edit_local_concept(&source, "info", "title", "Local title")
            .unwrap();
        source
            .edit_shared_concept("info", "description", "Description v2")
            .unwrap();
        let segment = resolve(&source, &local, &ReplaceComponent).unwrap();
        assert_eq!(
            segment.segment("info").unwrap().concepts[0].value,
            "Local title"
        );
        assert_eq!(
            segment.segment("info").unwrap().concepts[1].value,
            "Description v2"
        );
    }

    #[test]
    fn removing_point_difference_restores_dynamic_inheritance() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .edit_local_concept(&source, "info", "title", "Local title")
            .unwrap();
        local.remove_point_concept("info", "title");
        assert!(local.masks.is_empty());
        source
            .edit_shared_concept("info", "title", "Later title")
            .unwrap();
        assert_eq!(
            resolve(&source, &local, &ReplaceComponent)
                .unwrap()
                .segment("info")
                .unwrap()
                .concepts[0]
                .value,
            "Later title"
        );
    }

    #[test]
    fn reverting_to_the_inherited_value_removes_the_difference() {
        let source = source();
        let mut local = instance(&source);
        local
            .edit_local_concept(&source, "info", "title", "Local title")
            .unwrap();
        local
            .edit_local_concept(&source, "info", "title", "Original")
            .unwrap();
        assert!(local.masks.is_empty());
    }

    #[test]
    fn snapshot_freezes_only_explicitly_scoped_elements() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .create_snapshot(
                &source,
                "info",
                &[ElementAddress::Concept("title".to_owned())],
            )
            .unwrap();
        source
            .edit_shared_concept("info", "title", "Title v2")
            .unwrap();
        source
            .edit_shared_concept("info", "description", "Description v2")
            .unwrap();
        let segment = resolve(&source, &local, &ReplaceComponent).unwrap();
        assert_eq!(
            segment.segment("info").unwrap().concepts[0].value,
            "Original"
        );
        assert_eq!(
            segment.segment("info").unwrap().concepts[1].value,
            "Description v2"
        );
    }

    #[test]
    fn snapshot_rejects_source_removal_without_inventing_a_policy() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .create_snapshot(
                &source,
                "info",
                &[ElementAddress::Concept("title".to_owned())],
            )
            .unwrap();
        source
            .segment_mut("info")
            .unwrap()
            .concepts
            .retain(|item| item.key != "title");
        assert!(matches!(
            resolve(&source, &local, &ReplaceComponent),
            Err(MaskError::SnapshotSourceChanged { .. })
        ));
    }

    #[test]
    fn point_mask_rejects_source_removal_without_treating_it_as_a_tombstone() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .edit_local_concept(&source, "info", "title", "Local title")
            .unwrap();
        source
            .segment_mut("info")
            .unwrap()
            .concepts
            .retain(|item| item.key != "title");
        assert!(matches!(
            resolve(&source, &local, &ReplaceComponent),
            Err(MaskError::SourceElementMissing { .. })
        ));
    }

    #[test]
    fn snapshot_freezes_component_values_in_its_explicit_scope() {
        let mut source = source();
        let mut local = instance(&source);
        local
            .create_snapshot(
                &source,
                "info",
                &[ElementAddress::Component("link".to_owned())],
            )
            .unwrap();
        source.segment_mut("info").unwrap().components[0].value =
            json!({"url": "https://example.test/v2"});
        assert_eq!(
            resolve(&source, &local, &ReplaceComponent)
                .unwrap()
                .segment("info")
                .unwrap()
                .components[0]
                .value,
            json!({"url": "https://example.test/v1"})
        );
    }
    #[test]
    fn local_component_override_uses_the_component_merge_strategy() {
        let src = source();
        let mut local = instance(&src);
        local
            .edit_local_component(
                &src,
                "info",
                "link",
                json!({"url":"https://example.test/local"}),
            )
            .unwrap();
        let resolved = resolve(&src, &local, &ReplaceComponent).unwrap();
        assert_eq!(
            resolved.segment("info").unwrap().components[0].value,
            json!({"url":"https://example.test/local"})
        );
        assert_eq!(
            src.segment("info").unwrap().components[0].value,
            json!({"url":"https://example.test/v1"})
        );
    }

    #[test]
    fn snapshot_and_point_masks_cannot_override_the_same_element() {
        let src = source();
        let mut local = instance(&src);
        local
            .edit_local_concept(&src, "info", "title", "Local title")
            .unwrap();
        let err = local
            .create_snapshot(&src, "info", &[ElementAddress::Concept("title".into())])
            .unwrap_err();
        assert!(matches!(err, MaskError::OverlappingMasks { .. }));
    }
    #[test]
    fn source_catching_up_does_not_erase_an_existing_local_override() {
        let mut src = source();
        let mut local = instance(&src);
        local
            .edit_local_concept(&src, "info", "title", "Local title")
            .unwrap();
        src.edit_shared_concept("info", "title", "Local title")
            .unwrap();

        let resolved = resolve(&src, &local, &ReplaceComponent).unwrap();
        assert_eq!(
            resolved.segment("info").unwrap().concepts[0].value,
            "Local title"
        );
        assert_eq!(local.masks.len(), 1);
    }

    #[test]
    fn control_and_permission_segments_are_not_exposed_as_ordinary_visible_content() {
        let mut src = source();
        src.segments.push(Segment {
            role: SegmentRole::Control,
            key: "structure".to_owned(),
            concepts: vec![Concept {
                key: "enabled".to_owned(),
                value: "true".to_owned(),
            }],
            components: Vec::new(),
        });
        src.segments.push(Segment {
            role: SegmentRole::Permissions,
            key: "permissions".to_owned(),
            concepts: Vec::new(),
            components: Vec::new(),
        });

        let visible: Vec<_> = src
            .visible_segments()
            .map(|segment| segment.key.as_str())
            .collect();
        assert!(visible.contains(&"info"));
        assert!(!visible.contains(&"structure"));
        assert!(!visible.contains(&"permissions"));
    }
}
