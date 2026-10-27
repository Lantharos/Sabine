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
                walk(ty, &mut as_number);
            }
        });
        Ok(Cow::Owned(types))
    }

    fn map_type(&'_ self, _: &Types, dt: &DataType) -> Result<Cow<'_, DataType>, FormatError> {
        let mut dt = dt.clone();
        walk(&mut dt, &mut as_number);
        Ok(Cow::Owned(dt))
    }
}

fn as_number(dt: &mut DataType) {
    if let DataType::Primitive(primitive) = dt
        && !matches!(
            primitive,
            Primitive::bool | Primitive::str | Primitive::char
        )
    {
        *dt = specta_typescript::Number::<()>::definition(&mut Types::default());
    }
}
