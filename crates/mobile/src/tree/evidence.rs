use agent_desktop_core::{
    AccessibilityNode, ElementState, IdentifierEvidence, LiveElement, LiveIdentity,
    LocatorEvidence, LocatorField, LocatorRefEvidence, ObservedSubtree,
};

pub(crate) fn evidence(node: &AccessibilityNode) -> LocatorEvidence {
    LocatorEvidence {
        role: LocatorField::Known(node.role.clone()),
        name: field(node.identity.name.clone()),
        description: field(node.identity.description.clone()),
        value: field(node.identity.value.clone()),
        identifiers: IdentifierEvidence::typed(node.identity.native_id.clone(), Some(0), true),
        states: LocatorField::Known(node.presentation.states.clone()),
        ref_evidence: LocatorRefEvidence {
            window_bounds: field(node.presentation.bounds),
            bounds: field(node.presentation.bounds),
            available_actions: LocatorField::Known(node.presentation.available_actions.clone()),
            descriptors: node.presentation.descriptors.clone(),
        },
    }
}

pub(crate) fn subtree(node: &AccessibilityNode) -> ObservedSubtree {
    ObservedSubtree::new(
        evidence(node),
        node.children.iter().map(subtree).collect(),
        true,
        None,
    )
}

pub(crate) fn live(node: &AccessibilityNode) -> LiveElement {
    let evidence = evidence(node);
    let has = |state: &str| node.presentation.states.iter().any(|s| s == state);
    LiveElement {
        identity: LiveIdentity {
            name: evidence.name,
            description: evidence.description,
            identifiers: evidence.identifiers,
        },
        state: ElementState {
            role: node.role.clone(),
            states: node.presentation.states.clone(),
            value: node.identity.value.clone(),
            enabled: Some(!has("disabled")),
            hidden: Some(has("hidden")),
            offscreen: Some(has("offscreen")),
        },
        states_complete: true,
        bounds: node.presentation.bounds,
        available_actions: node.presentation.available_actions.clone(),
    }
}

fn field<T>(value: Option<T>) -> LocatorField<T> {
    value
        .map(LocatorField::Known)
        .unwrap_or(LocatorField::Absent)
}
