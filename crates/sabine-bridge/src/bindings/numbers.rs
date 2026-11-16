use std::borrow::Cow;

use specta::datatype::{DataType, Primitive};
use specta::{Format, FormatError, Type, Types};

use super::walk::walk;

/// Renders every Rust number as a TypeScript `number`, since that is what the
/// page's JSON parser makes of it.
pub(super) struct JsonNumbers;

impl Format for JsonNumbers {
    fn map_types(&'_ self, types: &Types) -> Result<Cow<'_, Types>, FormatError> {
        let mut types = types.clone();
        types.iter_mut(|named| {
            if let Some(ty) = &mut named.ty {
                as_numbers(ty);
            }
        });
        Ok(Cow::Owned(types))
    }

    fn map_type(&'_ self, _: &Types, dt: &DataType) -> Result<Cow<'_, DataType>, FormatError> {
        let mut dt = dt.clone();
        as_numbers(&mut dt);
        Ok(Cow::Owned(dt))
    }
}

fn as_numbers(dt: &mut DataType) {
    walk(dt, &mut |dt| {
        if matches!(dt, DataType::Primitive(primitive) if !matches!(primitive, Primitive::bool | Primitive::str | Primitive::char))
        {
            *dt = specta_typescript::Number::<()>::definition(&mut Types::default());
        }
    });
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use specta::Type;

    use crate::BridgeBindings;

    #[derive(Serialize, Type)]
    struct FileInfo {
        size: u64,
        modified: i64,
    }

    #[test]
    fn wide_integers_are_numbers() {
        let mut bindings = BridgeBindings::default();
        bindings.command::<(), FileInfo>("file.info");
        let ts = bindings.render().expect("bindings render");
        assert!(ts.contains("size: number"), "{ts}");
        assert!(ts.contains("modified: number"), "{ts}");
    }
}
