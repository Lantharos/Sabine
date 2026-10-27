use std::borrow::Cow;
use std::collections::HashSet;

use specta::datatype::{DataType, NamedDataType, NamedReferenceType, Reference};
use specta::{Format, Types};
use specta_serde::{Phase, PhasesFormat, select_phase_datatype};

use super::walk::walk;

const SERIALIZE: &str = "_Serialize";
const DESERIALIZE: &str = "_Deserialize";
const DISTINCT: &str = "__SabineDistinct";

pub(super) type TypeKey = (Cow<'static, str>, Cow<'static, str>);

/// Applies serde to `types` and gives every type one shape under its own name:
/// the shape the page receives when the app sends it, or else the shape the
/// app accepts.
pub(super) fn resolve(
    types: &Types,
    outputs: &mut [DataType],
    inputs: &mut [DataType],
) -> Result<Types, String> {
    let mut resolved = split_by_phase(types)?;
    for dt in outputs.iter_mut() {
        *dt = select_phase_datatype(dt, &resolved, Phase::Serialize);
    }
    for dt in inputs.iter_mut() {
        *dt = select_phase_datatype(dt, &resolved, Phase::Deserialize);
    }

    let received = reachable(&resolved, outputs.iter());
    let lookup = resolved.clone();
    let mut prefer_received = |dt: &mut DataType| {
        if let DataType::Reference(Reference::Named(reference)) = dt
            && let NamedReferenceType::Reference { generics, .. } = &reference.inner
            && let Some(accepted) = lookup.get(reference)
            && let Some(base) = accepted.name.strip_suffix(DESERIALIZE)
            && let Some(sent) = lookup.into_unsorted_iter().find(|named| {
                named.module_path == accepted.module_path
                    && named.name == format!("{base}{SERIALIZE}")
                    && received.contains(&key(named))
            })
        {
            *dt = DataType::Reference(sent.reference(generics.clone()));
        }
    };
    for dt in inputs.iter_mut() {
        walk(dt, &mut prefer_received);
    }
    resolved.iter_mut(|named| {
        if let Some(ty) = &mut named.ty {
            walk(ty, &mut prefer_received);
        }
    });

    let used = reachable(&resolved, outputs.iter().chain(inputs.iter()));
    let split = resolved
        .into_unsorted_iter()
        .filter_map(|named| {
            let base = named.name.strip_suffix(SERIALIZE)?;
            Some((named.module_path.clone(), Cow::Owned(base.to_string())))
        })
        .collect::<HashSet<_>>();
    resolved.iter_mut(|named| {
        let variant = [SERIALIZE, DESERIALIZE]
            .into_iter()
            .find_map(|suffix| named.name.strip_suffix(suffix))
            .map(str::to_string);
        match variant {
            Some(base) if used.contains(&key(named)) => named.name = Cow::Owned(base),
            Some(_) => named.ty = None,
            None if split.contains(&key(named)) => named.ty = None,
            None => {}
        }
    });
    Ok(resolved)
}

/// specta-serde tells split types apart by name alone, so each type carries a
/// distinct name while it splits them.
fn split_by_phase(types: &Types) -> Result<Types, String> {
    let mut names = Vec::new();
    let mut distinct = types.clone();
    distinct.iter_mut(|named| {
        let name = std::mem::take(&mut named.name);
        named.name = Cow::Owned(format!("{name}{DISTINCT}{}{DISTINCT}", names.len()));
        names.push(name);
    });
    let mut split = PhasesFormat
        .map_types(&distinct)
        .map_err(|error| error.to_string())?
        .into_owned();
    split.iter_mut(|named| {
        let original = named.name.split(DISTINCT).collect::<Vec<_>>();
        if let [_, index, suffix] = original[..]
            && let Some(name) = index
                .parse::<usize>()
                .ok()
                .and_then(|index| names.get(index))
        {
            named.name = Cow::Owned(format!("{name}{suffix}"));
        }
    });
    Ok(split)
}

fn reachable<'a>(types: &Types, roots: impl Iterator<Item = &'a DataType>) -> HashSet<TypeKey> {
    let mut reached = HashSet::new();
    let mut pending = roots.cloned().collect::<Vec<_>>();
    while let Some(mut dt) = pending.pop() {
        walk(&mut dt, &mut |dt| {
            if let DataType::Reference(Reference::Named(reference)) = dt
                && let Some(named) = types.get(reference)
                && reached.insert(key(named))
                && let Some(ty) = &named.ty
            {
                pending.push(ty.clone());
            }
        });
    }
    reached
}

pub(super) fn key(named: &NamedDataType) -> TypeKey {
    (named.module_path.clone(), named.name.clone())
}
