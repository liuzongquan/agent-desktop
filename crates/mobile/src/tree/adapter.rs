use crate::MobileAdapter;
use agent_desktop_core::{
    AccessibilityNode, AdapterError, AppInfo, Deadline, ElementState, LiveElement, NativeHandle,
    ObservationOps, ObservationRequest, ObservationRoot, ObservationSource, ObservedTree, Rect,
    RefEntry, TreeOptions, WindowFilter, WindowInfo,
};

impl ObservationOps for MobileAdapter {
    fn focused_app(&self, deadline: Deadline) -> Result<Option<AppInfo>, AdapterError> {
        self.current_app(deadline).map(Some)
    }

    fn list_apps(&self, deadline: Deadline) -> Result<Vec<AppInfo>, AdapterError> {
        self.current_app(deadline).map(|app| vec![app])
    }

    fn list_windows(
        &self,
        filter: &WindowFilter,
        deadline: Deadline,
    ) -> Result<Vec<WindowInfo>, AdapterError> {
        let window = self.window(deadline)?;
        if filter
            .app
            .as_ref()
            .is_some_and(|app| !window.app.eq_ignore_ascii_case(app))
        {
            return Ok(Vec::new());
        }
        Ok(vec![window])
    }

    fn get_tree(
        &self,
        win: &WindowInfo,
        _opts: &TreeOptions,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AdapterError> {
        self.verify_window(win, deadline)?;
        let tree = self.read_source(deadline)?;
        self.verify_window(win, deadline)?;
        Ok(tree)
    }

    fn observe_tree(
        &self,
        root: ObservationRoot<'_>,
        request: &ObservationRequest,
    ) -> Result<ObservedTree, AdapterError> {
        request.validate()?;
        let tree = match root {
            ObservationRoot::Window(window) => {
                self.get_tree(window, &TreeOptions::default(), request.deadline)?
            }
            ObservationRoot::Element { handle, .. } => {
                self.handle_node(handle, request.deadline)?
            }
            ObservationRoot::Application { .. } => {
                return Err(AdapterError::not_supported(
                    "application-owned mobile surface",
                ));
            }
        };
        enforce_budget(&tree, request, 0, &mut (0, 0))?;
        ObservedTree::from_roots(
            vec![super::evidence::subtree(&tree)],
            ObservationSource::from_root(&root, request.surface),
            Default::default(),
            true,
        )
    }

    fn resolve_element_strict(
        &self,
        entry: &RefEntry,
        deadline: Deadline,
    ) -> Result<NativeHandle, AdapterError> {
        if !entry.identity.unique_at_capture {
            return Err(AdapterError::ambiguous_target(
                "Mobile ref was not unique at capture",
            ));
        }
        self.resolve_entry(entry, deadline)
    }
    fn resolve_locator_anchor(
        &self,
        entry: &RefEntry,
        deadline: Deadline,
    ) -> Result<NativeHandle, AdapterError> {
        self.resolve_entry(entry, deadline)
    }
    fn get_live_element(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<LiveElement, AdapterError> {
        self.handle_node(handle, deadline)
            .map(|node| super::evidence::live(&node))
    }
    fn get_live_state(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<Option<ElementState>, AdapterError> {
        self.get_live_element(handle, deadline)
            .map(|live| Some(live.state))
    }
    fn get_live_value(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<Option<String>, AdapterError> {
        self.get_live_element(handle, deadline)
            .map(|live| live.state.value)
    }
    fn get_live_actions(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<Option<Vec<String>>, AdapterError> {
        self.get_live_element(handle, deadline)
            .map(|live| Some(live.available_actions))
    }
    fn get_element_bounds(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<Option<Rect>, AdapterError> {
        self.get_live_element(handle, deadline)
            .map(|live| live.bounds)
    }
}

fn enforce_budget(
    node: &AccessibilityNode,
    request: &ObservationRequest,
    depth: u8,
    counts: &mut (usize, usize),
) -> Result<(), AdapterError> {
    request.deadline.ensure_remaining()?;
    counts.0 += 1;
    let fields = [
        &node.identity.name,
        &node.identity.description,
        &node.identity.value,
    ];
    for field in fields.into_iter().flatten() {
        counts.1 += field.len();
        if field.len() > request.budget.max_field_bytes {
            return Err(AdapterError::timeout(
                "Mobile source field exceeds observation budget",
            ));
        }
    }
    if depth > 50
        || counts.0 > request.budget.max_nodes
        || counts.1 > request.budget.max_text_bytes
        || node.children.len() > request.budget.max_children_per_node
        || counts.0.saturating_sub(1) > request.budget.max_edges
    {
        return Err(AdapterError::timeout(
            "Mobile source exceeds observation budget",
        ));
    }
    for child in &node.children {
        enforce_budget(child, request, depth.saturating_add(1), counts)?;
    }
    Ok(())
}
