use std::{
    env,
    fs::File,
    io::{self, BufReader},
    net::IpAddr,
    path::Path,
    sync::Arc,
};

use rustls::{RootCertStore, pki_types::ServerName};
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

#[derive(Clone)]
pub struct TlsSettings {
    acceptor: TlsAcceptor,
    connector: TlsConnector,
}

impl TlsSettings {
    pub fn from_env() -> io::Result<Self> {
        Self::from_pem_files(
            env_path("ISHELL_CA_CERT")?,
            env_path("ISHELL_CERT")?,
            env_path("ISHELL_KEY")?,
        )
    }

    pub fn from_pem_files(
        ca_path: impl AsRef<Path>,
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let ca_certs = read_certs(ca_path)?;
        let certificates = read_certs(cert_path)?;
        let private_key = read_key(key_path)?;

        let mut roots = RootCertStore::empty();
        let (added, _) = roots.add_parsable_certificates(ca_certs);
        if added == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CA file contains no usable certificates",
            ));
        }

        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_client_auth_cert(certificates.clone(), private_key.clone_key())
            .map_err(io::Error::other)?;

        let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(io::Error::other)?;
        let server_config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certificates, private_key)
            .map_err(io::Error::other)?;

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(server_config)),
            connector: TlsConnector::from(Arc::new(client_config)),
        })
    }

    pub async fn accept(&self, stream: TcpStream) -> io::Result<TlsStream<TcpStream>> {
        Ok(self.acceptor.accept(stream).await?.into())
    }

    pub async fn connect(
        &self,
        stream: TcpStream,
        peer: IpAddr,
    ) -> io::Result<TlsStream<TcpStream>> {
        let server_name = ServerName::try_from(peer.to_string())
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(self.connector.connect(server_name, stream).await?.into())
    }
}

fn env_path(name: &str) -> io::Result<String> {
    env::var(name).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("environment variable {name} must point to a PEM file"),
        )
    })
}

fn read_certs(
    path: impl AsRef<Path>,
) -> io::Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut BufReader::new(File::open(path)?)).collect()
}

fn read_key(path: impl AsRef<Path>) -> io::Result<rustls::pki_types::PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut BufReader::new(File::open(path)?))?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "private-key file contains no supported PEM key",
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        path::PathBuf,
    };

    use rcgen::{
        BasicConstraints, Certificate, CertificateParams, DistinguishedName,
        ExtendedKeyUsagePurpose, IsCa, KeyPair,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_rustls::TlsStream;

    use super::TlsSettings;

    struct TestAuthority {
        directory: tempfile::TempDir,
        ca_path: PathBuf,
        certificate: Certificate,
        key: KeyPair,
    }

    impl TestAuthority {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let key = KeyPair::generate().unwrap();
            let mut params = CertificateParams::default();
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            let certificate = params.self_signed(&key).unwrap();
            let ca_path = directory.path().join("ca.pem");
            std::fs::write(&ca_path, certificate.pem()).unwrap();

            Self {
                directory,
                ca_path,
                certificate,
                key,
            }
        }

        fn settings_signed_by(
            &self,
            issuer: &TestAuthority,
            name: &str,
            ip: IpAddr,
        ) -> TlsSettings {
            let key = KeyPair::generate().unwrap();
            let mut params = CertificateParams::new(vec![ip.to_string()]).unwrap();
            params.extended_key_usages = vec![
                ExtendedKeyUsagePurpose::ClientAuth,
                ExtendedKeyUsagePurpose::ServerAuth,
            ];
            let mut distinguished_name = DistinguishedName::new();
            distinguished_name.push(rcgen::DnType::CommonName, name);
            params.distinguished_name = distinguished_name;
            let certificate = params
                .signed_by(&key, &issuer.certificate, &issuer.key)
                .unwrap();

            let cert_path = self.directory.path().join(format!("{name}.pem"));
            let key_path = self.directory.path().join(format!("{name}-key.pem"));
            std::fs::write(&cert_path, certificate.pem()).unwrap();
            std::fs::write(&key_path, key.serialize_pem()).unwrap();

            TlsSettings::from_pem_files(&self.ca_path, cert_path, key_path).unwrap()
        }
    }

    async fn connect_pair(
        server_settings: TlsSettings,
        client_settings: TlsSettings,
        listen_ip: IpAddr,
        verify_ip: IpAddr,
    ) -> (
        io::Result<TlsStream<TcpStream>>,
        io::Result<TlsStream<TcpStream>>,
    ) {
        let listener = TcpListener::bind(SocketAddr::new(listen_ip, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();

        tokio::join!(
            async move {
                let (stream, _) = listener.accept().await.unwrap();
                server_settings.accept(stream).await
            },
            async move {
                let stream = TcpStream::connect(address).await.unwrap();
                client_settings.connect(stream, verify_ip).await
            }
        )
    }

    #[tokio::test]
    async fn trusted_peers_exchange_control_and_stream_data_over_tls() {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let authority = TestAuthority::new();
        let server = authority.settings_signed_by(&authority, "server", ip);
        let client = authority.settings_signed_by(&authority, "client", ip);

        let (server, client) = connect_pair(server, client, ip, ip).await;
        let mut server = server.unwrap();
        let mut client = client.unwrap();

        crate::write_msg(
            &mut client,
            &super::super::control::NegotiationMessage::AttemptConnect,
        )
        .await
        .unwrap();
        let message: super::super::control::NegotiationMessage =
            crate::read_msg(&mut server).await.unwrap();
        assert_eq!(
            message,
            super::super::control::NegotiationMessage::AttemptConnect
        );

        client.write_all(b"private stream payload").await.unwrap();
        let mut payload = vec![0; b"private stream payload".len()];
        server.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload, b"private stream payload");
    }

    #[tokio::test]
    async fn rejects_server_certificate_for_a_different_ip() {
        let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let other_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let authority = TestAuthority::new();
        let server = authority.settings_signed_by(&authority, "server", localhost);
        let client = authority.settings_signed_by(&authority, "client", localhost);

        let (server_result, client_result) =
            connect_pair(server, client, localhost, other_ip).await;

        assert!(server_result.is_err());
        assert!(client_result.is_err());
    }

    #[tokio::test]
    async fn rejects_client_certificate_from_an_untrusted_authority() {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let trusted = TestAuthority::new();
        let untrusted = TestAuthority::new();
        let server = trusted.settings_signed_by(&trusted, "server", ip);
        let client = trusted.settings_signed_by(&untrusted, "client", ip);

        let (server_result, _) = connect_pair(server, client, ip, ip).await;

        assert!(server_result.is_err());
    }

    #[tokio::test]
    async fn rejects_server_certificate_from_an_untrusted_authority() {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let trusted = TestAuthority::new();
        let untrusted = TestAuthority::new();
        let server = untrusted.settings_signed_by(&untrusted, "server", ip);
        let client = trusted.settings_signed_by(&trusted, "client", ip);

        let (_, client_result) = connect_pair(server, client, ip, ip).await;

        assert!(client_result.is_err());
    }
}
