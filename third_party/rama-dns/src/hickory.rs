//! dns using the [`hickory_resolver`] crate

use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::sync::Arc;

pub use hickory_resolver as resolver;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::ResolverConfig;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::Name;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::proto::rr::rdata::A;
use hickory_resolver::proto::rr::rdata::AAAA;

use rama_core::error::ErrorContext;
use rama_core::error::OpaqueError;
use rama_core::telemetry::tracing;
use rama_net::address::Domain;
use rama_utils::macros::generate_set_and_with;

use crate::DnsResolver;

#[derive(Debug, Clone)]
/// [`DnsResolver`] using the [`hickory_resolver`] crate
pub struct HickoryDns(Arc<Result<TokioResolver, String>>);

impl Default for HickoryDns {
    #[cfg(any(target_family = "unix", target_os = "windows"))]
    fn default() -> Self {
        Self::try_new_system().unwrap_or_else(|err| {
            tracing::warn!(
                "fail to create system HickoryDns client: fallback to cloudflare: {err}"
            );
            Self::new_cloudflare()
        })
    }

    #[cfg(not(any(target_family = "unix", target_os = "windows")))]
    fn default() -> Self {
        Self::new_cloudflare()
    }
}

impl HickoryDns {
    #[inline]
    /// Construct a [`HickoryDnsBuilder`] used to build
    /// a custom [`HickoryDns`] instead of the default [`HickoryDns::new`].
    #[must_use]
    pub fn builder() -> HickoryDnsBuilder {
        HickoryDnsBuilder::default()
    }

    #[inline]
    /// Construct a new [`HickoryDns`] instance with the [`Default`] setup.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    /// Construct a new non-shared [`HickoryDns`] instance using Google's nameservers.
    ///
    /// Creates a default configuration, using `8.8.8.8`, `8.8.4.4` and `2001:4860:4860::8888`,
    /// `2001:4860:4860::8844` (thank you, Google).
    ///
    /// Please see Google's [privacy
    /// statement](https://developers.google.com/speed/public-dns/privacy) for important information
    /// about what they track, many ISP's track similar information in DNS.
    ///
    /// To use the system configuration see: [`Self::new_system`].
    pub fn new_google() -> Self {
        tracing::trace!("create HickoryDns resolver using default google config");
        Self::builder()
            .with_config(ResolverConfig::udp_and_tcp(&resolver::config::GOOGLE))
            .build()
    }

    #[inline]
    /// Construct a new non-shared [`HickoryDns`] instance using Cloudflare's nameservers.
    ///
    /// Creates a default configuration, using `1.1.1.1`, `1.0.0.1` and `2606:4700:4700::1111`, `2606:4700:4700::1001` (thank you, Cloudflare).
    ///
    /// Please see: <https://www.cloudflare.com/dns/>
    ///
    /// To use the system configuration see: [`Self::new_system`].
    pub fn new_cloudflare() -> Self {
        tracing::trace!("create HickoryDns resolver using default cloudflare config");
        Self::builder()
            .with_config(ResolverConfig::udp_and_tcp(&resolver::config::CLOUDFLARE))
            .build()
    }

    #[inline]
    /// Construct a new non-shared [`HickoryDns`] instance using Quad9's nameservers.
    ///
    /// Creates a configuration, using `9.9.9.9`, `149.112.112.112` and `2620:fe::fe`, `2620:fe::fe:9`,
    /// the "secure" variants of the quad9 settings (thank you, Quad9).
    ///
    /// Please see: <https://www.quad9.net/faq/>
    ///
    /// To use the system configuration see: [`Self::new_system`].
    pub fn new_quad9() -> Self {
        tracing::trace!("create HickoryDns resolver using default quad9 config");
        Self::builder()
            .with_config(ResolverConfig::udp_and_tcp(&resolver::config::QUAD9))
            .build()
    }

    #[cfg(any(target_family = "unix", target_os = "windows"))]
    /// Construct a new [`HickoryDns`] with the system configuration.
    ///
    /// This will use `/etc/resolv.conf` on Unix OSes and the registry on Windows.
    pub fn try_new_system() -> Result<Self, OpaqueError> {
        tracing::trace!("try to create HickoryDns resolver using system config");
        Ok(TokioResolver::builder_tokio()
            .context("build async dns resolver with system conf")
            .inspect_err(|err| {
                tracing::debug!("failed to create HickoryDns resolver using system config: {err:?}")
            })?
            .build()
            .context("build async dns resolver with system conf")?
            .into())
    }
}

impl From<TokioResolver> for HickoryDns {
    fn from(value: TokioResolver) -> Self {
        Self(Arc::new(Ok(value)))
    }
}

#[derive(Debug, Clone, Default)]
/// A [`Builder`] to [`build`][`Self::build`] a [`HickoryDns`] instance.
pub struct HickoryDnsBuilder {
    config: Option<self::resolver::config::ResolverConfig>,
    options: Option<self::resolver::config::ResolverOpts>,
}

impl HickoryDnsBuilder {
    generate_set_and_with! {
        /// Define the [`ResolverConfig`][`config::ResolverConfig`] used.
        pub fn config(mut self, config: Option<self::resolver::config::ResolverConfig>) -> Self {
            self.config = config;
            self
        }
    }

    generate_set_and_with! {
        /// Define the [`ResolverOpts`][`config::ResolverOpts`] used.
        #[must_use]
        pub fn options(mut self, options: Option<self::resolver::config::ResolverOpts>) -> Self {
            self.options = options;
            self
        }
    }

    /// Build a [`HickoryDns`] instance, consuming [`self`].
    ///
    /// [`Clone`] the [`HickoryDnsBuilder`] prior to calling this method in case you
    /// still need the builder afterwards.
    pub fn build(self) -> HickoryDns {
        let mut resolver_builder = TokioResolver::builder_with_config(
            self.config
                .unwrap_or_else(|| ResolverConfig::udp_and_tcp(&resolver::config::CLOUDFLARE)),
            TokioRuntimeProvider::default(),
        );
        if let Some(options) = self.options {
            *resolver_builder.options_mut() = options;
        }
        HickoryDns(Arc::new(
            resolver_builder.build().map_err(|error| error.to_string()),
        ))
    }
}

impl HickoryDns {
    // Preserve the existing infallible builder API. Configuration failures stay
    // attached to this resolver and are returned from lookups without a fallback.
    fn resolver(&self) -> Result<&TokioResolver, OpaqueError> {
        self.0
            .as_ref()
            .as_ref()
            .map_err(|error| OpaqueError::from_display(error.clone()))
    }
}

impl DnsResolver for HickoryDns {
    type Error = OpaqueError;

    async fn txt_lookup(&self, domain: Domain) -> Result<Vec<Vec<u8>>, Self::Error> {
        let name = fqdn_from_domain(domain)?;

        let mut results = vec![];
        for txt in self
            .resolver()?
            .txt_lookup(name)
            .await
            .context("lookup TXT entry")?
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::TXT(txt) => Some(txt),
                _ => None,
            })
        {
            for value in txt.txt_data.iter() {
                results.push(value.to_vec());
            }
        }
        Ok(results)
    }

    async fn ipv4_lookup(&self, domain: Domain) -> Result<Vec<Ipv4Addr>, Self::Error> {
        let name = fqdn_from_domain(domain)?;
        Ok(self
            .resolver()?
            .ipv4_lookup(name)
            .await
            .context("lookup IPv4 address(es)")?
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::A(A(ip)) => Some(*ip),
                _ => None,
            })
            .collect())
    }

    async fn ipv6_lookup(&self, domain: Domain) -> Result<Vec<Ipv6Addr>, Self::Error> {
        let name = fqdn_from_domain(domain)?;
        Ok(self
            .resolver()?
            .ipv6_lookup(name)
            .await
            .context("lookup IPv6 address(es)")?
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::AAAA(AAAA(ip)) => Some(*ip),
                _ => None,
            })
            .collect())
    }
}

fn fqdn_from_domain(domain: Domain) -> Result<Name, OpaqueError> {
    let mut name = Name::from_utf8(domain).context("try to consume a Domain as a Dns Name")?;
    name.set_fqdn(true);
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn builtin_resolver_configurations_remain_valid() {
        for resolver in [
            HickoryDns::new_google(),
            HickoryDns::new_cloudflare(),
            HickoryDns::new_quad9(),
        ] {
            assert!(resolver.resolver().is_ok());
        }
    }

    #[tokio::test]
    async fn configured_server_returns_ipv4_ipv6_and_txt_records() {
        use resolver::config::ConnectionConfig;
        use resolver::config::NameServerConfig;
        use resolver::proto::op::Message;
        use resolver::proto::op::OpCode;
        use resolver::proto::rr::Record;
        use resolver::proto::rr::RecordType;
        use resolver::proto::rr::rdata::TXT;
        use tokio::net::UdpSocket;

        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let mut buffer = [0; 4096];
                let (length, peer) = socket.recv_from(&mut buffer).await.unwrap();
                let request = Message::from_vec(&buffer[..length]).unwrap();
                let query = request.queries[0].clone();
                let data = match query.query_type() {
                    RecordType::A => RData::A(A(Ipv4Addr::new(192, 0, 2, 7))),
                    RecordType::AAAA => RData::AAAA(AAAA("2001:db8::7".parse().unwrap())),
                    RecordType::TXT => RData::TXT(TXT::new(vec!["fixture".into(), "text".into()])),
                    kind => panic!("unexpected fixture query: {kind:?}"),
                };
                let mut response = Message::response(request.metadata.id, OpCode::Query);
                response.edns = request.edns;
                response.add_query(query.clone());
                response.add_answer(Record::from_rdata(query.name().clone(), 60, data));
                socket
                    .send_to(&response.to_vec().unwrap(), peer)
                    .await
                    .unwrap();
            }
        });
        let mut connection = ConnectionConfig::udp();
        connection.port = address.port();
        let config = ResolverConfig::from_name_servers(vec![NameServerConfig::new(
            address.ip(),
            true,
            vec![connection],
        )]);
        let resolver = HickoryDns::builder().with_config(config).build();
        let domain: Domain = "lemex-resolver-fixture.example.com".parse().unwrap();
        assert_eq!(
            resolver.ipv4_lookup(domain.clone()).await.unwrap(),
            vec![Ipv4Addr::new(192, 0, 2, 7)]
        );
        assert_eq!(
            resolver.ipv6_lookup(domain.clone()).await.unwrap(),
            vec!["2001:db8::7".parse::<Ipv6Addr>().unwrap()]
        );
        assert_eq!(
            resolver.txt_lookup(domain).await.unwrap(),
            vec![b"fixture".to_vec(), b"text".to_vec()]
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn empty_custom_configuration_fails_without_falling_back() {
        let resolver = HickoryDns::builder()
            .with_config(ResolverConfig::from_name_servers(Vec::new()))
            .build();
        let domain: Domain = "lemex-resolver-fixture.example.com".parse().unwrap();
        assert!(resolver.ipv4_lookup(domain.clone()).await.is_err());
        assert!(resolver.ipv6_lookup(domain.clone()).await.is_err());
        assert!(resolver.txt_lookup(domain).await.is_err());
    }
}
