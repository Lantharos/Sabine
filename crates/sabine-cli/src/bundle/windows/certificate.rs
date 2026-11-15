// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The certificate's private key is persisted in the user's key storage while
// it is imported, because signtool runs in its own process. Every import has
// to remove the key, the certificates and the store it created again.

use std::path::Path;

use windows::{
    Win32::Security::Cryptography::{
        CERT_CONTEXT, CERT_KEY_PROV_INFO_PROP_ID, CERT_KEY_SPEC, CERT_OPEN_STORE_FLAGS,
        CERT_QUERY_ENCODING_TYPE, CERT_SHA1_HASH_PROP_ID, CERT_STORE_ADD_REPLACE_EXISTING,
        CERT_STORE_DELETE_FLAG, CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_CURRENT_USER,
        CRYPT_INTEGER_BLOB, CRYPT_KEY_PROV_INFO, CRYPT_USER_KEYSET,
        CertAddCertificateContextToStore, CertCloseStore, CertEnumCertificatesInStore,
        CertGetCertificateContextProperty, CertOpenStore, HCERTSTORE, NCRYPT_HANDLE,
        NCRYPT_KEY_HANDLE, NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG, NCryptDeleteKey,
        NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, PFXImportCertStore,
        PKCS12_ALWAYS_CNG_KSP,
    },
    core::HSTRING,
};

/// A `.pfx` certificate imported into a store of its own for the current
/// user, so signtool signs by thumbprint and never sees the password.
pub(super) struct ImportedCertificate {
    pub(super) store: String,
    pub(super) thumbprint: String,
    keys: Vec<(HSTRING, HSTRING)>,
}

impl ImportedCertificate {
    pub(super) fn import(path: &Path, password: Option<&str>) -> Result<Self, String> {
        let mut pfx = std::fs::read(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let blob = CRYPT_INTEGER_BLOB {
            cbData: pfx.len() as u32,
            pbData: pfx.as_mut_ptr(),
        };
        let password = HSTRING::from(password.unwrap_or_default());
        let mut name = [0_u8; 8];
        getrandom::fill(&mut name).map_err(|error| error.to_string())?;
        let mut certificate = Self {
            store: format!("Sabine signing {}", hex(&name)),
            thumbprint: String::new(),
            keys: Vec::new(),
        };
        let imported = unsafe {
            PFXImportCertStore(&blob, &password, CRYPT_USER_KEYSET | PKCS12_ALWAYS_CNG_KSP)
        }
        .map_err(|error| format!("could not import {}: {error}", path.display()))?;
        let copied = certificate.copy_from(imported);
        unsafe {
            let _ = CertCloseStore(Some(imported), 0);
        }
        copied?;
        if certificate.thumbprint.is_empty() {
            return Err("the .pfx certificate holds no private key".into());
        }
        Ok(certificate)
    }

    fn copy_from(&mut self, imported: HCERTSTORE) -> Result<(), String> {
        let store_name = HSTRING::from(&self.store);
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                CERT_QUERY_ENCODING_TYPE(0),
                None,
                CERT_OPEN_STORE_FLAGS(CERT_SYSTEM_STORE_CURRENT_USER),
                Some(store_name.as_ptr().cast()),
            )
        }
        .map_err(|error| format!("could not create a certificate store for signing: {error}"))?;
        let mut result = Ok(());
        let mut certificate: *mut CERT_CONTEXT = std::ptr::null_mut();
        loop {
            certificate = unsafe {
                CertEnumCertificatesInStore(
                    imported,
                    (!certificate.is_null()).then_some(certificate.cast_const()),
                )
            };
            if certificate.is_null() {
                break;
            }
            let Some(key) = key_name(certificate) else {
                result = result.and(add_certificate(store, certificate));
                continue;
            };
            self.keys.push(key);
            result = result
                .and(add_certificate(store, certificate))
                .and_then(|()| thumbprint(certificate))
                .map(|thumbprint| self.thumbprint = thumbprint);
        }
        unsafe {
            let _ = CertCloseStore(Some(store), 0);
        }
        result
    }
}

impl Drop for ImportedCertificate {
    fn drop(&mut self) {
        let store = HSTRING::from(&self.store);
        unsafe {
            let _ = CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                CERT_QUERY_ENCODING_TYPE(0),
                None,
                CERT_OPEN_STORE_FLAGS(CERT_SYSTEM_STORE_CURRENT_USER) | CERT_STORE_DELETE_FLAG,
                Some(store.as_ptr().cast()),
            );
        }
        for (provider, container) in &self.keys {
            if let Err(error) = delete_key(provider, container) {
                eprintln!("sabine: could not remove the imported signing key {container}: {error}");
            }
        }
    }
}

fn add_certificate(store: HCERTSTORE, certificate: *const CERT_CONTEXT) -> Result<(), String> {
    unsafe {
        CertAddCertificateContextToStore(
            Some(store),
            certificate,
            CERT_STORE_ADD_REPLACE_EXISTING,
            None,
        )
    }
    .map_err(|error| format!("could not add the signing certificate: {error}"))
}

fn thumbprint(certificate: *const CERT_CONTEXT) -> Result<String, String> {
    let mut hash = [0_u8; 20];
    let mut size = hash.len() as u32;
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_SHA1_HASH_PROP_ID,
            Some(hash.as_mut_ptr().cast()),
            &mut size,
        )
    }
    .map_err(|error| format!("could not read the certificate thumbprint: {error}"))?;
    Ok(hex(&hash))
}

/// The key storage provider and container of the certificate's private key.
fn key_name(certificate: *const CERT_CONTEXT) -> Option<(HSTRING, HSTRING)> {
    let mut size = 0;
    unsafe {
        CertGetCertificateContextProperty(certificate, CERT_KEY_PROV_INFO_PROP_ID, None, &mut size)
    }
    .ok()?;
    let mut buffer = vec![0_u64; (size as usize).div_ceil(8)];
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_KEY_PROV_INFO_PROP_ID,
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
        )
    }
    .ok()?;
    let info = unsafe { &*buffer.as_ptr().cast::<CRYPT_KEY_PROV_INFO>() };
    unsafe {
        Some((
            HSTRING::from_wide(info.pwszProvName.as_wide()),
            HSTRING::from_wide(info.pwszContainerName.as_wide()),
        ))
    }
}

fn delete_key(provider_name: &HSTRING, container: &HSTRING) -> windows::core::Result<()> {
    let mut provider = NCRYPT_PROV_HANDLE::default();
    unsafe { NCryptOpenStorageProvider(&mut provider, provider_name, 0) }?;
    let mut key = NCRYPT_KEY_HANDLE::default();
    let deleted = unsafe {
        NCryptOpenKey(
            provider,
            &mut key,
            container,
            CERT_KEY_SPEC(0),
            NCRYPT_SILENT_FLAG,
        )
        .and_then(|()| NCryptDeleteKey(key, 0))
    };
    unsafe {
        let _ = NCryptFreeObject(NCRYPT_HANDLE(provider.0));
    }
    deleted
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}
