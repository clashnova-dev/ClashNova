//! TLS 身份与配置（LAN-007）。
//!
//! 自签证书持久化于数据目录；fingerprint = 证书 DER 的 SHA-256（小写 hex），
//! 与 LocalSend HTTPS 模式一致。客户端用指纹 pin 对端证书（TOFU）：首次由用户
//! 审批确认，之后校验证书指纹是否与记录一致，不符即拒绝。

use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum_server::accept::Accept;
use axum_server::tls_rustls::{RustlsAcceptor, RustlsConfig};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::checksum::hex_lower;
use crate::error::{LanError, Result};

/// 本机 TLS 身份：自签证书 + 私钥（DER），以及证书指纹。
#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint: String,
}

impl Identity {
    /// 从数据目录加载（或首次生成并持久化）自签身份。
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        let cert_path = dir.join("device-cert.der");
        let key_path = dir.join("device-key.der");
        if let (Ok(cert_der), Ok(key_der)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
            let fingerprint = hex_lower(&Sha256::digest(&cert_der));
            return Ok(Self {
                cert_der,
                key_der,
                fingerprint,
            });
        }

        let certified = rcgen::generate_simple_self_signed(vec!["clashnova.local".to_string()])
            .map_err(|e| LanError::Config(format!("生成自签证书失败: {e}")))?;
        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.key_pair.serialize_der();
        let _ = std::fs::write(&cert_path, &cert_der);
        let _ = std::fs::write(&key_path, &key_der);
        let fingerprint = hex_lower(&Sha256::digest(&cert_der));
        Ok(Self {
            cert_der,
            key_der,
            fingerprint,
        })
    }
}

/// 接收端 TLS 配置（axum-server）。
///
/// 必须**显式指定 aws-lc-rs provider**构建 `ServerConfig`，不能用 `from_der`
/// （其内部依赖 rustls 的“进程默认 provider”）：本 App 同时链入了 ring（代理内核）
/// 与 aws-lc-rs 两套 provider，默认 provider 存在歧义，`ServerConfig::builder()`
/// 会 panic（"no process-level CryptoProvider"），导致接收端静默起不来。
///
/// **强制 mTLS**：对端必须出示客户端证书，否则 TLS 握手失败。证书不链到任何 CA
/// （双方都是自签），因此 `verify_client_cert` 放行任意证书，但 rustls 仍校验握手
/// 签名——证明对端持有该证书私钥。信任判定在应用层用“已验证证书指纹”做 TOFU
/// （见 [`PeerCertAcceptor`]），不再采信请求体里对端自报的指纹。
pub async fn server_config(identity: &Identity) -> Result<RustlsConfig> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let certs = vec![CertificateDer::from(identity.cert_der.clone())];
    let key = PrivateKeyDer::Pkcs8(identity.key_der.clone().into());
    let verifier = Arc::new(AnyClientCertVerifier {
        provider: provider.clone(),
    });
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| LanError::Config(e.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(|e| LanError::Config(e.to_string()))?;
    Ok(RustlsConfig::from_config(Arc::new(config)))
}

/// 构造 LAN 传输用的 HTTP(S) 客户端。
///
/// 永远绕过系统代理（局域网直连，见设计文档 LAN-009；也避免连本机地址被代理拦截）。
/// `use_tls` 时按指纹 pin 对端自签证书：`expected_fingerprint` 为 `Some` 则校验对端
/// 证书 SHA-256 必须一致，为 `None` 则仅加密不校验身份（首次探测，如拉取 /info）。
///
/// 接收端现在**强制 mTLS**，因此 `use_tls` 下必须出示本机身份证书 `identity`：接收端
/// 据此验证的证书指纹判断信任。`identity` 为 `None`（仅测试的明文/匿名场景）则不出示。
pub fn client(
    use_tls: bool,
    expected_fingerprint: Option<String>,
    identity: Option<&Identity>,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3));
    if use_tls {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = Arc::new(FingerprintVerifier {
            expected: expected_fingerprint,
            provider: provider.clone(),
        });
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| LanError::Config(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier);
        let config = match identity {
            Some(id) => config
                .with_client_auth_cert(
                    vec![CertificateDer::from(id.cert_der.clone())],
                    PrivateKeyDer::Pkcs8(id.key_der.clone().into()),
                )
                .map_err(|e| LanError::Config(e.to_string()))?,
            None => config.with_no_client_auth(),
        };
        builder = builder.use_preconfigured_tls(config);
    }
    builder.build().map_err(LanError::from)
}

/// 自签证书场景下的证书校验器：仅按 SHA-256 指纹 pin 对端证书，
/// 并用加密提供者校验握手签名（证明对端持有该证书私钥）。
#[derive(Debug)]
struct FingerprintVerifier {
    expected: Option<String>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, TlsError> {
        if let Some(expected) = &self.expected {
            let actual = hex_lower(&Sha256::digest(end_entity.as_ref()));
            if &actual != expected {
                return Err(TlsError::General("对端证书指纹不匹配".to_string()));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// 强制 mTLS 的客户端证书校验器：自签场景无 CA 可链，放行任意**结构合法**的证书，
/// 由 rustls 校验握手签名证明对端持私钥；对端身份（指纹 TOFU）在应用层判定。
#[derive(Debug)]
struct AnyClientCertVerifier {
    provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for AnyClientCertVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, TlsError> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// 经 TLS 握手验证过的对端客户端证书指纹（SHA-256，小写 hex）。放进请求扩展，
/// 供处理器据此判断信任，取代对端自报的 `info.fingerprint`。
#[derive(Clone, Debug)]
pub struct PeerFingerprint(pub String);

/// 把每条连接的[已验证指纹]注入请求扩展的服务包装。
#[derive(Clone)]
pub struct WithPeerFingerprint<S> {
    inner: S,
    fp: Option<String>,
}

impl<S, B> tower_service::Service<axum::http::Request<B>> for WithPeerFingerprint<S>
where
    S: tower_service::Service<axum::http::Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: axum::http::Request<B>) -> Self::Future {
        if let Some(fp) = &self.fp {
            req.extensions_mut().insert(PeerFingerprint(fp.clone()));
        }
        self.inner.call(req)
    }
}

/// 在 `RustlsAcceptor` 之上握手完成后取出已验证的客户端证书，算出指纹并注入请求扩展。
/// 这是 mTLS 的落点：接收端由此拿到**密码学可信**的对端身份。
#[derive(Clone)]
pub struct PeerCertAcceptor {
    inner: RustlsAcceptor,
}

impl PeerCertAcceptor {
    pub fn new(config: RustlsConfig) -> Self {
        Self {
            inner: RustlsAcceptor::new(config),
        }
    }
}

impl<S> Accept<TcpStream, S> for PeerCertAcceptor
where
    S: Send + 'static,
{
    type Stream = TlsStream<TcpStream>;
    type Service = WithPeerFingerprint<S>;
    type Future = Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send>>;

    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let (stream, service) = inner.accept(stream, service).await?;
            let fp = stream
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|certs| certs.first())
                .map(|cert| hex_lower(&Sha256::digest(cert.as_ref())));
            Ok((stream, WithPeerFingerprint { inner: service, fp }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_persistent_with_hex_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let first = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(first.fingerprint.len(), 64);
        assert!(first.fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
        // 再次加载应得到同一身份（持久化）。
        let second = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.cert_der, second.cert_der);
    }
}
