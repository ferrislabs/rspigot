use std::sync::Arc;

use rustls::ServerConfig;
use rustls::crypto::ring::default_provider;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;

use crate::ManagementError;

pub fn acceptor_from_pem(path: &str) -> Result<TlsAcceptor, ManagementError> {
    let tls = |what: &str, error: &dyn std::fmt::Display| {
        ManagementError::Tls(format!("{what} in {path}: {error}"))
    };
    let certificates: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(path)
        .map_err(|e| tls("cannot read the certificates", &e))?
        .collect::<Result<_, _>>()
        .map_err(|e| tls("invalid certificate", &e))?;
    if certificates.is_empty() {
        return Err(ManagementError::Tls(format!(
            "no certificate found in {path}"
        )));
    }
    let key = PrivateKeyDer::from_pem_file(path).map_err(|e| tls("no usable private key", &e))?;
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| tls("unsupported protocol set", &e))?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|e| tls("certificate and key do not match", &e))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}
