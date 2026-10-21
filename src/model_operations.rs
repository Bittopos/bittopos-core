//! Reusable, read-only operations over the workspace model.

use uuid::Uuid;

use crate::{Instance, Workspace, WorkspaceModelError};

/// Returns the direct children of a workspace location.
///
/// `None` represents the workspace root; `Some(parent_id)` returns only that
/// instance's immediate children, not all descendants.
pub fn children_of(
    workspace: &Workspace,
    parent_id: Option<Uuid>,
) -> Result<&[Instance], WorkspaceModelError> {
    match parent_id {
        None => Ok(workspace.instances()),
        Some(parent_id) => workspace
            .find_instance(parent_id)
            .map(Instance::children)
            .ok_or(WorkspaceModelError::ParentNotFound(parent_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_of_root_returns_only_root_instances() {
        let workspace = Workspace::new();
        assert!(children_of(&workspace, None).unwrap().is_empty());
    }

    #[test]
    fn children_of_returns_only_immediate_children_at_each_depth() {
        let resource_id = Uuid::now_v7();
        let root_id = Uuid::now_v7();
        let child_id = Uuid::now_v7();
        let grandchild_id = Uuid::now_v7();

        let mut child = Instance::new(child_id, resource_id);
        child.content = Some(crate::Content {
            instances: vec![Instance::new(grandchild_id, resource_id)],
        });
        let mut root = Instance::new(root_id, resource_id);
        root.content = Some(crate::Content {
            instances: vec![child],
        });
        let workspace = Workspace {
            content: crate::Content {
                instances: vec![root],
            },
        };

        assert_eq!(children_of(&workspace, None).unwrap()[0].id, root_id);
        assert_eq!(
            children_of(&workspace, Some(root_id)).unwrap()[0].id,
            child_id
        );
        assert_eq!(
            children_of(&workspace, Some(child_id)).unwrap()[0].id,
            grandchild_id
        );
    }

    #[test]
    fn children_of_missing_parent_returns_domain_error() {
        let workspace = Workspace::new();
        let missing_id = Uuid::now_v7();
        assert_eq!(
            children_of(&workspace, Some(missing_id)),
            Err(WorkspaceModelError::ParentNotFound(missing_id))
        );
    }
}
