//! TLS for the hosts a world's parts serve: each branch has its own CA, which its drivers and
//! agents are told to trust through env variables (never a system trust store), and the proxy
//! makes a leaf for each host from it.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use chrono::Datelike;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use tokio_rustls::rustls::{
    self,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};

/// How many years a world's CA (and its leaves) stay valid.
const VALID_YEARS: i32 = 10;

/// A branch's CA: its certificate in `pem_path` (what drivers and agents trust) and the leaves
/// made from it, one per host. Its folder is removed when it is dropped.
pub struct WorldCa {
    cert: Certificate,
    key: KeyPair,
    dir: PathBuf,
    pem: PathBuf,
    leaves: Mutex<HashMap<String, Arc<rustls::ServerConfig>>>,
}

/// The validity of a certificate made now: from the start of this year for `VALID_YEARS`.
fn validity(p: &mut CertificateParams) {
    let year = chrono::Utc::now().year();
    p.not_before = rcgen::date_time_ymd(year, 1, 1);
    p.not_after = rcgen::date_time_ymd(year + VALID_YEARS, 1, 1);
}

impl WorldCa {
    /// A new CA "Shakedown world <branch>", its PEM written to `dir/ca.pem`.
    pub fn new(branch: &str, dir: &Path) -> anyhow::Result<WorldCa> {
        let mut p = CertificateParams::new(Vec::<String>::new())?;
        p.distinguished_name
            .push(DnType::CommonName, format!("Shakedown world {branch}"));
        p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        validity(&mut p);
        let key = KeyPair::generate()?;
        let cert = p.self_signed(&key)?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let pem = dir.join("ca.pem");
        std::fs::write(&pem, cert.pem()).with_context(|| format!("writing {}", pem.display()))?;
        let leaves = Mutex::default();
        let dir = dir.to_path_buf();
        Ok(WorldCa {
            cert,
            key,
            dir,
            pem,
            leaves,
        })
    }

    /// The CA's certificate (PEM): what drivers and agents are told to trust.
    pub fn pem_path(&self) -> &Path {
        &self.pem
    }

    /// The TLS server config for `host`: a leaf for it signed by the CA (made once), ALPN
    /// `http/1.1`.
    pub fn server_config(&self, host: &str) -> anyhow::Result<Arc<rustls::ServerConfig>> {
        if let Some(c) = self.leaves.lock().unwrap().get(host) {
            return Ok(c.clone());
        }
        let config = Arc::new(self.leaf_config(host)?);
        self.leaves
            .lock()
            .unwrap()
            .insert(host.to_string(), config.clone());
        Ok(config)
    }

    fn leaf_config(&self, host: &str) -> anyhow::Result<rustls::ServerConfig> {
        let mut p = CertificateParams::new(vec![host.to_string()])?;
        p.distinguished_name.push(DnType::CommonName, host);
        validity(&mut p);
        let key = KeyPair::generate()?;
        let leaf = p.signed_by(&key, &self.cert, &self.key)?;
        let chain: Vec<CertificateDer<'static>> = vec![leaf.der().clone(), self.cert.der().clone()];
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(chain, key)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(config)
    }
}

impl Drop for WorldCa {
    fn drop(&mut self) {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                eprintln!("proxy: removing {}: {e}", self.dir.display());
            }
            _ => {}
        }
    }
}

/// Every branch's CA, made on first use.
#[derive(Default)]
pub struct Cas {
    by_branch: Mutex<HashMap<String, Arc<WorldCa>>>,
}

impl Cas {
    /// The CA of `branch` (in `<temp>/arena-proxy/<branch>/`), made the first time it is asked
    /// for. A fork is a branch of its own: its world gets its own CA.
    pub fn for_branch(&self, branch: &str) -> anyhow::Result<Arc<WorldCa>> {
        let mut by_branch = self.by_branch.lock().unwrap();
        if let Some(ca) = by_branch.get(branch) {
            return Ok(ca.clone());
        }
        let dir = std::env::temp_dir().join("arena-proxy").join(branch);
        let ca = Arc::new(WorldCa::new(branch, &dir)?);
        by_branch.insert(branch.to_string(), ca.clone());
        Ok(ca)
    }

    /// Forgets `branch`'s CA (its folder goes once no tunnel holds it).
    pub fn drop_branch(&self, branch: &str) {
        self.by_branch.lock().unwrap().remove(branch);
    }

    /// Forgets every branch's CA (at shutdown: their folders go once no tunnel holds them).
    pub fn clear(&self) {
        self.by_branch.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use rustls::pki_types::pem::PemObject;

    use super::*;

    fn rustls_pemfile_certs(pem: &[u8]) -> Vec<CertificateDer<'static>> {
        CertificateDer::pem_slice_iter(pem)
            .map(Result::unwrap)
            .collect()
    }

    /// A TLS handshake in process: a client that trusts only `ca` connects to `server` as `name`.
    async fn handshake(ca: &WorldCa, server: Arc<rustls::ServerConfig>, name: &str) -> bool {
        let pem = std::fs::read(ca.pem_path()).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_pemfile_certs(&pem) {
            roots.add(cert).unwrap();
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let (client_io, server_io) = tokio::io::duplex(1 << 16);
        let acceptor = tokio_rustls::TlsAcceptor::from(server);
        tokio::spawn(async move { acceptor.accept(server_io).await.map(drop) });
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from(name.to_string()).unwrap();
        connector.connect(name, client_io).await.is_ok()
    }

    #[tokio::test]
    async fn the_leaf_names_its_host() {
        let dir = std::env::temp_dir().join(format!("arena-tls-test-{}", std::process::id()));
        let ca = WorldCa::new("b_leaf", &dir).unwrap();
        let server = ca.server_config("toy.test").unwrap();
        assert!(handshake(&ca, server.clone(), "toy.test").await);
        assert!(!handshake(&ca, server, "other.test").await);
        drop(ca);
        assert!(!dir.exists());
    }

    #[test]
    fn each_branch_has_its_own_ca() {
        let cas = Cas::default();
        let (a, b) = (
            cas.for_branch("b_one").unwrap(),
            cas.for_branch("b_two").unwrap(),
        );
        let read = |ca: &WorldCa| std::fs::read(ca.pem_path()).unwrap();
        assert_ne!(read(&a), read(&b));
        assert_eq!(cas.for_branch("b_one").unwrap().pem_path(), a.pem_path());
        let dirs = [
            a.pem_path().parent().unwrap().to_path_buf(),
            b.pem_path().parent().unwrap().to_path_buf(),
        ];
        drop((a, b));
        cas.drop_branch("b_one");
        cas.drop_branch("b_two");
        assert!(dirs.iter().all(|d| !d.exists()));
    }

    #[test]
    fn clearing_removes_every_branchs_ca() {
        let cas = Cas::default();
        let pem = cas.for_branch("b_clear").unwrap().pem_path().to_path_buf();
        assert!(pem.exists());
        cas.clear();
        assert!(!pem.parent().unwrap().exists());
    }
}
