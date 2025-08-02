use color_eyre::eyre;
use kuma_backend::spawn;
use kuma_core::config::Config;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let config = Config::load()?;

    let res = spawn(config).await?;

    Ok(res)
}
