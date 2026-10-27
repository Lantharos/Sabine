use specta::datatype::{DataType, Fields, NamedReferenceType, Reference};

/// Calls `visit` on `dt` and then on every type nested in it.
pub(super) fn walk(dt: &mut DataType, visit: &mut impl FnMut(&mut DataType)) {
    visit(dt);
    match dt {
        DataType::Primitive(_)
        | DataType::Generic(_)
        | DataType::Reference(Reference::Opaque(_)) => {}
        DataType::List(list) => walk(&mut list.ty, visit),
        DataType::Map(map) => {
            walk(map.key_ty_mut(), visit);
            walk(map.value_ty_mut(), visit);
        }
        DataType::Struct(structure) => walk_fields(&mut structure.fields, visit),
        DataType::Enum(enumeration) => {
            for (_, variant) in &mut enumeration.variants {
                walk_fields(&mut variant.fields, visit);
            }
        }
        DataType::Tuple(tuple) => tuple.elements.iter_mut().for_each(|dt| walk(dt, visit)),
        DataType::Nullable(inner) => walk(inner, visit),
        DataType::Intersection(parts) => parts.iter_mut().for_each(|dt| walk(dt, visit)),
        DataType::Reference(Reference::Named(named)) => match &mut named.inner {
            NamedReferenceType::Inline { dt, .. } => walk(dt, visit),
            NamedReferenceType::Reference { generics, .. } => {
                generics.iter_mut().for_each(|(_, dt)| walk(dt, visit));
            }
            NamedReferenceType::Recursive(_) => {}
        },
    }
}

fn walk_fields(fields: &mut Fields, visit: &mut impl FnMut(&mut DataType)) {
    let types: Box<dyn Iterator<Item = _>> = match fields {
        Fields::Unit => return,
        Fields::Unnamed(unnamed) => Box::new(unnamed.fields.iter_mut()),
        Fields::Named(named) => Box::new(named.fields.iter_mut().map(|(_, field)| field)),
    };
    for field in types {
        if let Some(ty) = &mut field.ty {
            walk(ty, visit);
        }
    }
}
