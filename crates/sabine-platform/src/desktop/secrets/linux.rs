use std::collections::HashMap;

use futures_util::StreamExt;
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Proxy, Variant};

const SERVICE: &str = "org.freedesktop.secrets";
const PATH: &str = "/org/freedesktop/secrets";
const SERVICE_INTERFACE: &str = "org.freedesktop.Secret.Service";
const COLLECTION_INTERFACE: &str = "org.freedesktop.Secret.Collection";
const ITEM_INTERFACE: &str = "org.freedesktop.Secret.Item";
const PROMPT_INTERFACE: &str = "org.freedesktop.Secret.Prompt";
const SCHEMA: &str = "rs.sabine.Secret";
const DEFAULT_COLLECTION: &str = "default";
const NO_OBJECT: &str = "/";
const CONTENT_TYPE: &str = "text/plain; charset=utf8";

/// A secret as the Secret Service carries it: its session, the session's
/// parameters, the value and its content type.
type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

pub(super) async fn set(app_id: &str, key: &str, value: &str) -> Result<(), String> {
    let keyring = Keyring::open().await?;
    let collection = keyring.default_collection().await?;
    let properties = HashMap::from([
        (
            "org.freedesktop.Secret.Item.Label",
            Variant::from(super::label(app_id, key)),
        ),
        (
            "org.freedesktop.Secret.Item.Attributes",
            Variant::from(attributes(app_id, key)),
        ),
    ]);
    let secret = (
        &keyring.session,
        Vec::<u8>::new(),
        value.as_bytes(),
        CONTENT_TYPE,
    );
    let (_, prompt): (OwnedObjectPath, OwnedObjectPath) = collection
        .call("CreateItem", &(properties, secret, true))
        .await
        .map_err(refused)?;
    keyring.prompt(prompt).await
}

pub(super) async fn get(app_id: &str, key: &str) -> Result<Option<String>, String> {
    let keyring = Keyring::open().await?;
    let (unlocked, locked) = keyring.search(app_id, key).await?;
    let item = match (unlocked.into_iter().next(), locked.into_iter().next()) {
        (Some(item), _) => item,
        (None, Some(item)) => {
            keyring.unlock(vec![item.clone()]).await?;
            item
        }
        (None, None) => return Ok(None),
    };
    let (_, _, value, _): Secret = keyring
        .proxy(item, ITEM_INTERFACE)
        .await?
        .call("GetSecret", &(&keyring.session,))
        .await
        .map_err(refused)?;
    String::from_utf8(value)
        .map(Some)
        .map_err(|_| "The keyring holds a secret under this key that is not text".to_string())
}

pub(super) async fn delete(app_id: &str, key: &str) -> Result<bool, String> {
    let keyring = Keyring::open().await?;
    let (unlocked, locked) = keyring.search(app_id, key).await?;
    let mut deleted = false;
    for item in unlocked.into_iter().chain(locked) {
        let prompt: OwnedObjectPath = keyring
            .proxy(item, ITEM_INTERFACE)
            .await?
            .call("Delete", &())
            .await
            .map_err(refused)?;
        keyring.prompt(prompt).await?;
        deleted = true;
    }
    Ok(deleted)
}

fn attributes<'a>(app_id: &'a str, key: &'a str) -> HashMap<&'static str, &'a str> {
    HashMap::from([
        ("xdg:schema", SCHEMA),
        ("application", app_id),
        ("key", key),
    ])
}

/// The person's keyring through the Secret Service, with a session that
/// carries secrets over the session bus as they are, where only the person's
/// own processes can read them.
struct Keyring {
    connection: Connection,
    service: Proxy<'static>,
    session: OwnedObjectPath,
}

impl Keyring {
    async fn open() -> Result<Self, String> {
        let connection = Connection::session().await.map_err(unavailable)?;
        let service = Proxy::new(&connection, SERVICE, PATH, SERVICE_INTERFACE)
            .await
            .map_err(unavailable)?;
        let (_, session): (OwnedVariant, OwnedObjectPath) = service
            .call("OpenSession", &("plain", Variant::from("")))
            .await
            .map_err(unavailable)?;
        Ok(Self {
            connection,
            service,
            session,
        })
    }

    async fn proxy(
        &self,
        path: OwnedObjectPath,
        interface: &'static str,
    ) -> Result<Proxy<'static>, String> {
        Proxy::new(&self.connection, SERVICE, path, interface)
            .await
            .map_err(refused)
    }

    /// The collection new secrets go in, unlocked.
    async fn default_collection(&self) -> Result<Proxy<'static>, String> {
        let path: OwnedObjectPath = self
            .service
            .call("ReadAlias", &(DEFAULT_COLLECTION,))
            .await
            .map_err(refused)?;
        if path.as_str() == NO_OBJECT {
            return Err("The keyring has no default collection to keep secrets in".to_string());
        }
        let collection = self.proxy(path.clone(), COLLECTION_INTERFACE).await?;
        let locked: bool = collection.get_property("Locked").await.map_err(refused)?;
        if locked {
            self.unlock(vec![path]).await?;
        }
        Ok(collection)
    }

    async fn search(
        &self,
        app_id: &str,
        key: &str,
    ) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), String> {
        self.service
            .call("SearchItems", &(attributes(app_id, key),))
            .await
            .map_err(refused)
    }

    async fn unlock(&self, objects: Vec<OwnedObjectPath>) -> Result<(), String> {
        let (_, prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) = self
            .service
            .call("Unlock", &(objects,))
            .await
            .map_err(refused)?;
        self.prompt(prompt).await
    }

    /// Lets the keyring ask the person, such as for the password that
    /// unlocks it, and waits for their answer.
    async fn prompt(&self, prompt: OwnedObjectPath) -> Result<(), String> {
        if prompt.as_str() == NO_OBJECT {
            return Ok(());
        }
        let prompt = self.proxy(prompt, PROMPT_INTERFACE).await?;
        let mut completed = prompt.receive_signal("Completed").await.map_err(refused)?;
        prompt
            .call::<_, _, ()>("Prompt", &("",))
            .await
            .map_err(refused)?;
        let message = completed
            .next()
            .await
            .ok_or("The keyring stopped answering")?;
        let (dismissed, _): (bool, OwnedVariant) = message.body().deserialize().map_err(refused)?;
        if dismissed {
            return Err("The keyring was not unlocked".to_string());
        }
        Ok(())
    }
}

fn unavailable(error: zbus::Error) -> String {
    format!(
        "No Secret Service answers on the session bus, so secrets can't be kept. Run a keyring such as GNOME Keyring, KWallet or KeePassXC: {error}"
    )
}

fn refused(error: impl std::fmt::Display) -> String {
    format!("The keyring refused: {error}")
}
