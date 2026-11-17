use std::net::SocketAddr;

/// Настройки сервиса. Всё приходит из окружения (deploy/.env) или флагов CLI.
#[derive(Debug, Clone, clap::Args)]
pub struct Config {
    /// SQLite-база, например `sqlite:///data/app.db`.
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite://data/app.db")]
    pub database_url: String,

    /// Где слушать HTTP.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080")]
    pub bind_addr: SocketAddr,

    #[command(flatten)]
    pub livekit: LiveKitConfig,

    /// Сколько доверенных reverse-proxy стоит перед бэкендом. 0 — IP клиента = адрес TCP-пира;
    /// N — берём N-й адрес с конца заголовка X-Forwarded-For (его дописал ближайший доверенный прокси).
    #[arg(long, env = "TRUSTED_PROXY_HOPS", default_value_t = 0)]
    pub trusted_proxy_hops: usize,

    /// Каждое входящее соединение начинается с заголовка PROXY protocol v2 (standalone: Caddy L4 → API).
    /// Тогда IP клиента берётся из него; соединения без заголовка отбрасываются.
    #[arg(long, env = "PROXY_PROTOCOL", default_value = "false", action = clap::ArgAction::Set,
          value_parser = clap::builder::BoolishValueParser::new())]
    pub proxy_protocol: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct LiveKitConfig {
    /// Внешний адрес LiveKit, который получают клиенты (wss://lk.example.com).
    #[arg(long, env = "LIVEKIT_PUBLIC_URL", default_value = "")]
    pub public_url: String,
    #[arg(
        long,
        env = "LIVEKIT_API_KEY",
        default_value = "",
        hide_env_values = true
    )]
    pub api_key: String,
    #[arg(
        long,
        env = "LIVEKIT_API_SECRET",
        default_value = "",
        hide_env_values = true
    )]
    pub api_secret: String,
}

impl LiveKitConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.public_url.is_empty(), "LIVEKIT_PUBLIC_URL не задан");
        anyhow::ensure!(!self.api_key.is_empty(), "LIVEKIT_API_KEY не задан");
        anyhow::ensure!(!self.api_secret.is_empty(), "LIVEKIT_API_SECRET не задан");
        Ok(())
    }
}
