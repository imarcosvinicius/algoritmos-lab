use super::place::{read_at_path, ScopeFrame, SlotBinding};
use super::{DebugSnapshot, MAX_SNAPSHOT_ARRAY_ITEMS};
use crate::value::VarSnapshot;

pub(crate) fn snapshot_binding_entry(binding: &SlotBinding, scope: &str) -> VarSnapshot {
    let display_name = binding.display_name().to_string();
    let scope_name = scope.to_string();
    match binding {
        SlotBinding::Slot { slot, .. } => {
            let s = slot.borrow();
            let (items, total_items) = s.value.snapshot_details(MAX_SNAPSHOT_ARRAY_ITEMS);
            let type_label = if s.is_const {
                format!("const {}", s.value.type_label())
            } else {
                s.value.type_label()
            };
            VarSnapshot {
                name: display_name,
                type_label,
                value_text: s.value.to_display_string(),
                scope: scope_name,
                items,
                total_items,
            }
        }
        SlotBinding::SubPlace {
            declared_type,
            slot,
            steps,
            ..
        } => {
            let s = slot.borrow();
            if let Ok(val) = read_at_path(&s.value, &display_name, steps) {
                let (items, total_items) = val.snapshot_details(MAX_SNAPSHOT_ARRAY_ITEMS);
                VarSnapshot {
                    name: display_name,
                    type_label: val.type_label(),
                    value_text: val.to_display_string(),
                    scope: scope_name,
                    items,
                    total_items,
                }
            } else {
                VarSnapshot {
                    name: display_name,
                    type_label: declared_type.type_label(),
                    value_text: "<indisponível>".to_string(),
                    scope: scope_name,
                    items: Vec::new(),
                    total_items: None,
                }
            }
        }
    }
}

pub(crate) fn snapshot_vars_from(globals: &ScopeFrame, frames: &[ScopeFrame]) -> Vec<VarSnapshot> {
    let mut out = Vec::new();
    for key in &globals.order {
        if let Some(binding) = globals.bindings.get(key) {
            out.push(snapshot_binding_entry(binding, &globals.label));
        }
    }
    if let Some(frame) = frames.last() {
        for key in &frame.order {
            if let Some(binding) = frame.bindings.get(key) {
                out.push(snapshot_binding_entry(binding, &frame.label));
            }
        }
    }
    out
}

pub(crate) fn snapshot_debug_from(
    program_name: &str,
    globals: &ScopeFrame,
    frames: &[ScopeFrame],
) -> DebugSnapshot {
    let mut call_stack = Vec::with_capacity(frames.len() + 1);
    call_stack.push(format!("algoritmo \"{program_name}\""));
    for frame in frames {
        call_stack.push(frame.label.clone());
    }
    DebugSnapshot {
        variables: snapshot_vars_from(globals, frames),
        call_stack,
    }
}
