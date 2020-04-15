use crate::lang::serialization::{Serializable, DeserializationState, SerializationState};
use crate::lang::serialization::model::{Element, element};
use crate::lang::serialization::model;
use crate::lang::errors::{CrushResult, error};
use crate::lang::value::Value;
use crate::lang::scope::Scope;
use crate::util::identity_arc::Identity;

impl Serializable<Scope> for Scope {
    fn deserialize(id: usize, elements: &Vec<Element>, state: &mut DeserializationState) -> CrushResult<Scope> {
        if state.scopes.contains_key(&id) {
            Ok(state.scopes[&id].clone())
        } else {
            match elements[id].element.as_ref().unwrap() {
                element::Element::UserScope(s) => {
                    let name = match s.name {
                        None | Some(model::scope::Name::HasName(_)) => None,
                        Some(model::scope::Name::NameValue(n)) => Some(String::deserialize(n as usize, elements, state)?.into_boxed_str()),
                    };
                    let res = Scope::create(name, s.is_loop, s.is_stopped, s.is_readonly);
                    state.scopes.insert(id, res.clone());
                    if let Some(model::scope::Parent::ParentValue(pid)) = s.parent {
                        res.set_parent(Some(Scope::deserialize(pid as usize, elements, state)?));
                    }
                    if let Some(model::scope::Calling::CallingValue(cid)) = s.calling {
                        res.set_calling(Some(Scope::deserialize(cid as usize, elements, state)?));
                    }
                    for uid in &s.uses {
                        res.r#use(&Scope::deserialize(*uid as usize, elements, state)?);
                    }
                    for mid in s.members.iter() {
                        match &elements[*mid as usize].element {
                            Some(model::element::Element::Member(m)) => {
                                res.redeclare(
                                    &String::deserialize(m.name as usize, elements, state)?,
                                    Value::deserialize(m.value as usize, elements, state)?,
                                )?;
                            }
                            _ => {
                                return error("Invalid scope member");
                            }
                        }
                    }
                    Ok(res)
                }
                element::Element::InternalScope(s) => {
                    match state.env.global_value(
                        s.elements
                            .iter()
                            .map(|s| s.clone().into_boxed_str())
                            .collect()) {
                        Ok(Value::Scope(s)) => Ok(s),
                        Ok(_) => error("Value is not a scope"),
                        Err(_) => error("Invalid path for scope lookup"),
                    }
                }
                _ => error("Expected a scope"),
            }
        }
    }

    fn serialize(&self, elements: &mut Vec<Element>, state: &mut SerializationState) -> CrushResult<usize> {
        let id = self.id();
        if !state.with_id.contains_key(&id) {
            let idx = elements.len();
            elements.push(model::Element::default());
            state.with_id.insert(id, idx);

            match self.full_path() {
                Ok(p) => {
                    elements[idx] = model::Element {
                        element: Some(model::element::Element::InternalScope(
                            model::Strings { elements: p.iter().map(|s| s.to_string()).collect() }))
                    };
                }
                Err(_) => {
                    let mut sscope: model::Scope = model::Scope::default();
                    let scope_data = self.export();

                    match scope_data.name {
                        None => {
                            sscope.name = Some(model::scope::Name::HasName(false));
                        }
                        Some(n) => {
                            let nid = n.to_string().serialize(elements, state)?;
                            sscope.name = Some(model::scope::Name::NameValue(nid as u64));
                        }
                    }
                    match scope_data.parent_scope {
                        None => {
                            sscope.parent = Some(model::scope::Parent::HasParent(false));
                        }
                        Some(p) => {
                            let pid = p.serialize(elements, state)?;
                            sscope.parent = Some(model::scope::Parent::ParentValue(pid as u64));
                        }
                    }
                    match scope_data.calling_scope {
                        None => {
                            sscope.calling = Some(model::scope::Calling::HasCalling(false));
                        }
                        Some(c) => {
                            let cid = c.serialize(elements, state)?;
                            sscope.calling = Some(model::scope::Calling::CallingValue(cid as u64));
                        }
                    }
                    sscope.is_readonly = scope_data.is_readonly;
                    sscope.is_loop = scope_data.is_loop;
                    sscope.is_stopped = scope_data.is_stopped;

                    for u in scope_data.uses.iter() {
                        sscope.uses.push(u.serialize(elements, state)? as u64);
                    }

                    for (k, v) in scope_data.mapping.iter() {
                        let name_idx = k.to_string().serialize(elements, state)?;
                        let value_idx = v.serialize(elements, state)?;

                        let entry_idx = elements.len();
                        elements.push(model::Element {
                            element: Some(model::element::Element::Member(
                                model::Member {
                                    name: name_idx as u64,
                                    value: value_idx as u64,
                                }
                            ))
                        });

                        sscope.members.push(entry_idx as u64);
                    }

                    elements[idx] = model::Element {
                        element: Some(model::element::Element::UserScope(sscope)),
                    };
                }
            }
        }
        Ok(state.with_id[&id])
    }
}
