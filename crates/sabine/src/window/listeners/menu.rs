use std::{fmt, sync::Arc};

use crate::{MenuActivation, MenuKind, WindowId};

/// What a window host writes when someone chooses an app's menu item.
pub(crate) const MENU_ITEM_LINE: &str = "SABINE_MENU_ITEM";

/// The app's listener for the menu items people choose.
#[derive(Clone)]
pub(crate) struct MenuItemListener(Arc<dyn Fn(MenuActivation) + Send + Sync>);

impl MenuItemListener {
    pub(crate) fn new(listener: impl Fn(MenuActivation) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(super) fn deliver(&self, window: WindowId, line: &str) {
        #[derive(serde::Deserialize)]
        struct Chosen {
            id: String,
            menu: MenuKind,
        }
        let Some(Ok(chosen)) = line
            .split_once('\t')
            .map(|(_, chosen)| serde_json::from_str::<Chosen>(chosen))
        else {
            return;
        };
        (self.0)(MenuActivation {
            window,
            id: chosen.id,
            menu: chosen.menu,
        });
    }
}

/// The line a window host writes when someone chooses item `id`.
pub(crate) fn menu_item_line(id: &str, menu: MenuKind) -> String {
    format!(
        "{MENU_ITEM_LINE}\t{}",
        serde_json::json!({ "id": id, "menu": menu })
    )
}

impl fmt::Debug for MenuItemListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MenuItemListener")
    }
}
