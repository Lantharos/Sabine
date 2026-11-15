use std::borrow::Cow;

use specta::datatype::{DataType, Primitive};
use specta::{Format, FormatError, Type, Types};

use super::walk::walk;

/// Renders every Rust number as a TypeScript `number`, since that is what the
/// page's JSON parser makes of it, and refuses integers wider than the 53 bits
/// a `number` holds exactly.
pub(super) struct JsonNumbers;

impl Format for JsonNumbers {
    fn map_types(&'_ self, types: &Types) -> Result<Cow<'_, Types>, FormatError> {
        let mut types = types.clone();
        let mut error = None;
        types.iter_mut(|named| {
            if let Some(ty) = &mut named.ty
                && error.is_none()
                && let Err(wide) = as_numbers(ty)
            {
                error = Some(too_wide(wide, &format!("`{}`", named.name)));
            }
        });
        match error {
            Some(error) => Err(error.into()),
            None => Ok(Cow::Owned(types)),
        }
    }

    fn map_type(&'_ self, _: &Types, dt: &DataType) -> Result<Cow<'_, DataType>, FormatError> {
        let mut dt = dt.clone();
        as_numbers(&mut dt).map_err(|wide| too_wide(wide, "a command or event"))?;
        Ok(Cow::Owned(dt))
    }
}

fn as_numbers(dt: &mut DataType) -> Result<(), Primitive> {
    let mut wide = None;
    walk(dt, &mut |dt| {
        let DataType::Primitive(primitive) = dt else {
            return;
        };
        match primitive {
            Primitive::bool | Primitive::str | Primitive::char => {}
            Primitive::i64
            | Primitive::u64
            | Primitive::i128
            | Primitive::u128
            | Primitive::isize
            | Primitive::usize => {
                wide.get_or_insert(primitive.clone());
            }
            _ => *dt = specta_typescript::Number::<()>::definition(&mut Types::default()),
        }
    });
    wide.map_or(Ok(()), Err)
}

fn too_wide(wide: Primitive, owner: &str) -> String {
    format!(
        "{owner} holds a `{wide:?}`, which the page reads as a JavaScript number that is exact only \
         up to 2^53. Use an integer of 32 bits or fewer, or send it as a string: serialize it with \
         serde_with's `DisplayFromStr` and mark the field `#[specta(type = String)]`"
    )
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use specta::Type;

    use crate::BridgeBindings;

    #[derive(Serialize, Type)]
    struct Wide {
        size: u64,
    }

    #[derive(Serialize, Type)]
    struct AsString {
        #[specta(type = String)]
        size: u64,
    }

    fn render<T: Type>() -> Result<String, String> {
        let mut bindings = BridgeBindings::default();
        bindings.command::<(), T>("file.size");
        bindings.render()
    }

    #[test]
    fn wide_integers_need_another_representation() {
        let error = render::<Wide>().expect_err("u64 is refused");
        assert!(error.contains("`Wide` holds a `u64`"), "{error}");
        assert!(render::<Vec<i64>>().is_err());
        assert!(render::<AsString>().is_ok_and(|ts| ts.contains("size: string")));
    }
}
